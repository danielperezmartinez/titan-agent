use crate::{config::Config, discovery::CodexClient, snapshot::snapshot_ansi};
use alacritty_terminal::{
    event::{OnResize, VoidListener, WindowSize},
    term::{Term, test::TermSize},
    tty::{self, ChildEvent, EventedPty, Options, Shell},
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufReader, Read, Write},
    net::Shutdown,
    os::unix::{
        fs::DirBuilderExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use titan_agent_core::{
    discovery::{
        fallback_project, project_from_git_common_dir, session_from_codex_thread,
        sessions_from_claude_agents,
    },
    protocol::*,
    session::AgentCli,
};

struct State {
    sessions: Vec<titan_agent_core::protocol::Session>,
    subscribers: Vec<Arc<Mutex<UnixStream>>>,
    clients: Vec<Arc<Mutex<UnixStream>>>,
    terminals: HashMap<String, Arc<Terminal>>,
}
struct Terminal {
    pty: Arc<Mutex<tty::Pty>>,
    term: Arc<Mutex<Term<VoidListener>>>,
    clients: Arc<Mutex<Vec<Arc<Mutex<UnixStream>>>>>,
}
pub struct Daemon;
pub struct DaemonHandle {
    socket: PathBuf,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
    accept: Option<thread::JoinHandle<()>>,
    poll: Option<thread::JoinHandle<()>>,
}
impl Daemon {
    #[allow(clippy::collapsible_if)]
    pub fn start(config: Config) -> std::io::Result<DaemonHandle> {
        let parent = config.socket.parent().unwrap_or(Path::new("."));
        let mut b = fs::DirBuilder::new();
        b.recursive(true).mode(0o700);
        b.create(parent)?;
        if config.socket.exists() {
            if UnixStream::connect(&config.socket).is_ok() {
                return Err(std::io::Error::from(std::io::ErrorKind::AddrInUse));
            }
            let _ = fs::remove_file(&config.socket);
        }
        let listener = UnixListener::bind(&config.socket)?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(State {
            sessions: vec![],
            subscribers: vec![],
            clients: vec![],
            terminals: HashMap::new(),
        }));
        let ac = Arc::clone(&stop);
        let ast = Arc::clone(&state);
        let cfg = config.clone();
        let accept = thread::spawn(move || {
            while !ac.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let s = Arc::clone(&ast);
                        let c = cfg.clone();
                        thread::spawn(move || client(stream, s, c));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        let ps = Arc::clone(&stop);
        let pst = Arc::clone(&state);
        let pc = config.clone();
        let poll = thread::spawn(move || {
            let mut old = Vec::new();
            let mut old_claude = Vec::new();
            let projects = std::cell::RefCell::new(HashMap::<
                String,
                titan_agent_core::protocol::Project,
            >::new());
            while !ps.load(Ordering::Relaxed) {
                let mut claude_sessions = old_claude.clone();
                let output = Command::new(&pc.claude_program)
                    .args(["agents", "--json"])
                    .stdin(Stdio::null())
                    .output();
                if let Ok(o) = output {
                    if o.status.success() {
                        if let Ok(text) = String::from_utf8(o.stdout) {
                            let result = sessions_from_claude_agents(&text, &|cwd| {
                                project(cwd, &pc, &mut projects.borrow_mut())
                            });
                            if let Ok(v) = result {
                                claude_sessions = v
                            }
                        }
                    }
                } else {
                    eprintln!("claude discovery failed")
                }
                old_claude = claude_sessions.clone();
                let mut sessions = claude_sessions;
                if let Some(program) = &pc.codex_program {
                    if let Ok(mut child) = Command::new(program)
                        .args(["app-server", "proxy"])
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .spawn()
                    {
                        if let (Some(input), Some(output)) =
                            (child.stdin.take(), child.stdout.take())
                        {
                            let mut c = CodexClient::new(BufReader::new(output), input);
                            let r = c
                                .initialize(env!("CARGO_PKG_VERSION"))
                                .and_then(|_| c.loaded_threads());
                            if let Ok(threads) = r {
                                sessions.extend(threads.iter().filter_map(|t| {
                                    session_from_codex_thread(t, &|cwd| {
                                        project(cwd, &pc, &mut projects.borrow_mut())
                                    })
                                }))
                            }
                            let _ = child.kill();
                        }
                    }
                }
                let mut s = pst.lock().unwrap();
                if sessions != old {
                    s.sessions = sessions.clone();
                    old = sessions.clone();
                    for stream in &mut s.subscribers {
                        let _ = stream.lock().unwrap().write_all(
                            encode_line(&Message::Notification(
                                DaemonNotification::SessionsChanged {
                                    sessions: sessions.clone(),
                                },
                            ))
                            .as_bytes(),
                        );
                    }
                }
                drop(s);
                thread::sleep(if pst.lock().unwrap().subscribers.is_empty() {
                    pc.poll_idle
                } else {
                    pc.poll_subscribed
                });
            }
        });
        Ok(DaemonHandle {
            socket: config.socket,
            stop,
            state,
            accept: Some(accept),
            poll: Some(poll),
        })
    }
}
impl DaemonHandle {
    pub fn socket(&self) -> &Path {
        &self.socket
    }
    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.accept.take() {
            let _ = h.join();
        }
        if let Some(h) = self.poll.take() {
            let _ = h.join();
        }
        let mut state = self.state.lock().unwrap();
        state.subscribers.clear();
        for client in &state.clients {
            let _ = client.lock().unwrap().shutdown(Shutdown::Both);
        }
        state.clients.clear();
        state.terminals.clear();
        let _ = fs::remove_file(&self.socket);
    }
}
pub fn run(config: Config) -> std::io::Result<()> {
    let _handle = Daemon::start(config)?;
    loop {
        thread::park()
    }
}
fn project(
    cwd: &str,
    cfg: &Config,
    cache: &mut HashMap<String, titan_agent_core::protocol::Project>,
) -> titan_agent_core::protocol::Project {
    if let Some(v) = cache.get(cwd) {
        return v.clone();
    }
    let has = |p: &str| Path::new(p).join(".git").exists();
    let p = cfg
        .git_program
        .as_ref()
        .and_then(|g| {
            Command::new(g)
                .args([
                    "-C",
                    cwd,
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-common-dir",
                ])
                .output()
                .ok()
        })
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| project_from_git_common_dir(s.trim()))
        .unwrap_or_else(|| fallback_project(cwd, &has));
    cache.insert(cwd.into(), p.clone());
    p
}
fn client(stream: UnixStream, state: Arc<Mutex<State>>, config: Config) {
    let writer = Arc::new(Mutex::new(stream.try_clone().unwrap()));
    let reader = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    state.lock().unwrap().clients.push(Arc::clone(&writer));
    let mut lines = LineReader::new(BufReader::new(reader));
    let mut hello = false;
    let mut opened = HashSet::new();
    loop {
        match lines.next_line() {
            Ok(Some(line)) => match decode_line(&line) {
                Ok(Message::Request { id, request }) => {
                    let (res, close) = handle(
                        &writer,
                        &state,
                        &config,
                        &mut hello,
                        &mut opened,
                        id,
                        request,
                    );
                    if !res {
                        break;
                    }
                    if close {
                        break;
                    }
                }
                Ok(_) => {}
                Err(DecodeError::UnknownMethod { id: Some(id), .. }) => {
                    reply(
                        &writer,
                        id,
                        Err((error_code::UNKNOWN_METHOD, "unknown method".into())),
                    );
                }
                Err(DecodeError::InvalidParams {
                    id: Some(id),
                    message,
                }) => reply(&writer, id, Err((error_code::INVALID_PARAMS, message))),
                Err(DecodeError::InvalidJson(_) | DecodeError::TooLong) => break,
                Err(_) => {}
            },
            Ok(None) => break,
            Err(_) => break,
        }
    }
    let mut state = state.lock().unwrap();
    state.subscribers.retain(|s| !Arc::ptr_eq(s, &writer));
    state.clients.retain(|s| !Arc::ptr_eq(s, &writer));
    for terminal in state.terminals.values() {
        terminal
            .clients
            .lock()
            .unwrap()
            .retain(|s| !Arc::ptr_eq(s, &writer));
    }
}
fn reply(w: &Arc<Mutex<UnixStream>>, id: u64, result: Result<Value, (i32, String)>) {
    let r = result.map_err(|(code, message)| RpcError { code, message });
    let _ = w
        .lock()
        .unwrap()
        .write_all(encode_line(&Message::Response { id, result: r }).as_bytes());
}
fn terminal_for(
    state: &Arc<Mutex<State>>,
    session: &str,
    writer: &Arc<Mutex<UnixStream>>,
) -> Option<Arc<Terminal>> {
    let state = state.lock().unwrap();
    let terminal = state.terminals.get(session)?.clone();
    if terminal
        .clients
        .lock()
        .unwrap()
        .iter()
        .any(|c| Arc::ptr_eq(c, writer))
    {
        Some(terminal)
    } else {
        None
    }
}
fn terminal_reader(
    mut reader: std::fs::File,
    session: String,
    term: Arc<Mutex<Term<VoidListener>>>,
    clients: Arc<Mutex<Vec<Arc<Mutex<UnixStream>>>>>,
    pty: std::sync::Weak<Mutex<tty::Pty>>,
    state: Arc<Mutex<State>>,
) {
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
    let mut processor = Processor::<StdSyncHandler>::new();
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break,
            Ok(n) => {
                processor.advance(&mut *term.lock().unwrap(), &buffer[..n]);
                let message =
                    encode_line(&Message::Notification(DaemonNotification::TerminalOutput {
                        session: session.clone(),
                        data: buffer[..n].to_vec(),
                    }));
                let mut clients = clients.lock().unwrap();
                clients
                    .retain(|client| client.lock().unwrap().write_all(message.as_bytes()).is_ok());
            }
        }
    }
    let mut code = None;
    if let Some(pty) = pty.upgrade() {
        for _ in 0..20 {
            if let Some(ChildEvent::Exited(status)) = pty.lock().unwrap().next_child_event() {
                code = status.and_then(|status| status.code());
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    let clients = clients.lock().unwrap().clone();
    let message = encode_line(&Message::Notification(DaemonNotification::TerminalExited {
        session: session.clone(),
        code,
    }));
    for client in clients {
        let _ = client.lock().unwrap().write_all(message.as_bytes());
    }
    state.lock().unwrap().terminals.remove(&session);
}
fn handle(
    w: &Arc<Mutex<UnixStream>>,
    state: &Arc<Mutex<State>>,
    cfg: &Config,
    hello: &mut bool,
    opened: &mut HashSet<String>,
    id: u64,
    r: ClientRequest,
) -> (bool, bool) {
    let err = |c: i32, m: &str| Err((c, m.to_string()));
    if !*hello && !matches!(r, ClientRequest::Hello { .. }) {
        reply(w, id, err(error_code::HELLO_REQUIRED, "hello required"));
        return (true, false);
    }
    let result = match r {
        ClientRequest::Hello { protocol, .. } => {
            if protocol != PROTOCOL_VERSION {
                reply(
                    w,
                    id,
                    err(error_code::UNSUPPORTED_PROTOCOL, "unsupported protocol"),
                );
                return (true, true);
            }
            *hello = true;
            Ok(
                json!({"protocol":PROTOCOL_VERSION,"daemon":{"name":"titan-agentd","version":env!("CARGO_PKG_VERSION")}}),
            )
        }
        ClientRequest::SessionsList => Ok(json!({"sessions":state.lock().unwrap().sessions})),
        ClientRequest::SessionsSubscribe => {
            state.lock().unwrap().subscribers.push(Arc::clone(w));
            Ok(json!({"sessions":state.lock().unwrap().sessions}))
        }
        ClientRequest::TerminalOpen {
            session,
            cols,
            rows,
        } => {
            if cols == 0 || rows == 0 {
                {
                    reply(w, id, err(5, "invalid dimensions"));
                    return (true, false);
                }
            }
            let ses = state
                .lock()
                .unwrap()
                .sessions
                .iter()
                .find(|s| s.id == session)
                .cloned();
            match ses {
                None => err(3, "unknown session"),
                Some(s) if s.attach.is_none() => err(4, "session cannot be attached"),
                Some(s) => {
                    let mut reader_file = None;
                    let existing = state.lock().unwrap().terminals.get(&session).cloned();
                    let terminal = if let Some(t) = existing {
                        let mut pty = t.pty.lock().unwrap();
                        pty.on_resize(WindowSize {
                            num_lines: rows,
                            num_cols: cols,
                            cell_width: 0,
                            cell_height: 0,
                        });
                        drop(pty);
                        t.term
                            .lock()
                            .unwrap()
                            .resize(TermSize::new(cols as usize, rows as usize));
                        t
                    } else {
                        let program = if s.cli == AgentCli::ClaudeCode {
                            cfg.claude_program.clone()
                        } else {
                            cfg.codex_program.clone().unwrap_or_else(|| "codex".into())
                        };
                        let mut args = s.attach.clone().unwrap();
                        if !args.is_empty() {
                            args.remove(0);
                        }
                        let options = Options {
                            shell: Some(Shell::new(program, args)),
                            working_directory: Some(s.cwd.into()),
                            env: HashMap::from([
                                ("TERM".into(), "xterm-256color".into()),
                                ("COLORTERM".into(), "truecolor".into()),
                            ]),
                            ..Options::default()
                        };
                        match tty::new(
                            &options,
                            WindowSize {
                                num_lines: rows,
                                num_cols: cols,
                                cell_width: 0,
                                cell_height: 0,
                            },
                            0,
                        ) {
                            Ok(pty) => {
                                let term = Arc::new(Mutex::new(Term::new(
                                    Default::default(),
                                    &TermSize::new(cols as usize, rows as usize),
                                    VoidListener,
                                )));
                                let clients = Arc::new(Mutex::new(Vec::new()));
                                let pty = Arc::new(Mutex::new(pty));
                                let terminal = Arc::new(Terminal {
                                    pty: Arc::clone(&pty),
                                    term: Arc::clone(&term),
                                    clients: Arc::clone(&clients),
                                });
                                let file = match terminal.pty.lock().unwrap().file().try_clone() {
                                    Ok(file) => file,
                                    Err(error) => {
                                        reply(w, id, err(7, &error.to_string()));
                                        return (true, false);
                                    }
                                };
                                state
                                    .lock()
                                    .unwrap()
                                    .terminals
                                    .insert(session.clone(), Arc::clone(&terminal));
                                reader_file = Some((file, term, clients, Arc::downgrade(&pty)));
                                terminal
                            }
                            Err(e) => {
                                reply(w, id, err(7, &e.to_string()));
                                return (true, false);
                            }
                        }
                    };
                    let mut clients = terminal.clients.lock().unwrap();
                    if !clients.iter().any(|c| Arc::ptr_eq(c, w)) {
                        clients.push(Arc::clone(w));
                    }
                    opened.insert(session.clone());
                    let snap = snapshot_ansi(&terminal.term.lock().unwrap());
                    let mut writer = w.lock().unwrap();
                    let _ = writer.write_all(
                        encode_line(&Message::Response {
                            id,
                            result: Ok(json!({})),
                        })
                        .as_bytes(),
                    );
                    let _ = writer.write_all(
                        encode_line(&Message::Notification(DaemonNotification::TerminalOutput {
                            session: session.clone(),
                            data: snap,
                        }))
                        .as_bytes(),
                    );
                    drop(writer);
                    drop(clients);
                    if let Some((file, term, clients, pty_ref)) = reader_file {
                        let st = Arc::clone(state);
                        let name = session.clone();
                        thread::spawn(move || {
                            terminal_reader(file, name, term, clients, pty_ref, st)
                        });
                    }
                    return (true, false);
                }
            }
        }
        ClientRequest::TerminalInput { session, data } => terminal_for(state, &session, w)
            .map_or_else(
                || err(3, "terminal not attached"),
                |t| {
                    let p = t.pty.lock().unwrap();
                    p.file()
                        .write_all(&data)
                        .map(|_| json!({}))
                        .map_err(|e| (7, e.to_string()))
                },
            ),
        ClientRequest::TerminalResize {
            session,
            cols,
            rows,
        } => {
            if cols == 0 || rows == 0 {
                reply(w, id, err(5, "invalid dimensions"));
                return (true, false);
            }
            terminal_for(state, &session, w).map_or_else(
                || err(3, "terminal not attached"),
                |t| {
                    t.pty.lock().unwrap().on_resize(WindowSize {
                        num_lines: rows,
                        num_cols: cols,
                        cell_width: 0,
                        cell_height: 0,
                    });
                    t.term
                        .lock()
                        .unwrap()
                        .resize(TermSize::new(cols as usize, rows as usize));
                    Ok(json!({}))
                },
            )
        }
        ClientRequest::TerminalClose { session } => terminal_for(state, &session, w).map_or_else(
            || err(3, "terminal not attached"),
            |t| {
                t.clients.lock().unwrap().retain(|c| !Arc::ptr_eq(c, w));
                opened.remove(&session);
                Ok(json!({}))
            },
        ),
        ClientRequest::SessionStop { session } => {
            let ses = state
                .lock()
                .unwrap()
                .sessions
                .iter()
                .find(|s| s.id == session)
                .cloned();
            match ses {
                None => err(3, "unknown session"),
                Some(s) if s.stop.is_none() => err(4, "stop unavailable"),
                Some(s) => {
                    if s.cli == AgentCli::Codex {
                        let Some(program) = cfg.codex_program.as_ref() else {
                            return (reply(w, id, err(4, "stop unavailable")), (true, false)).1;
                        };
                        let Some(thread_id) = session.strip_prefix("codex:") else {
                            return (
                                reply(w, id, err(7, "invalid Codex session id")),
                                (true, false),
                            )
                                .1;
                        };
                        let outcome = (|| -> Result<(), String> {
                            let mut child = Command::new(program)
                                .args(["app-server", "proxy"])
                                .stdin(Stdio::piped())
                                .stdout(Stdio::piped())
                                .stderr(Stdio::null())
                                .spawn()
                                .map_err(|e| e.to_string())?;
                            let result = if let (Some(input), Some(output)) =
                                (child.stdin.take(), child.stdout.take())
                            {
                                let mut client = CodexClient::new(BufReader::new(output), input);
                                client
                                    .initialize(env!("CARGO_PKG_VERSION"))
                                    .and_then(|_| client.interrupt(thread_id))
                                    .map(|_| ())
                                    .map_err(|e| format!("{e:?}"))
                            } else {
                                Err("Codex app-server pipes unavailable".into())
                            };
                            let _ = child.kill();
                            let _ = child.wait();
                            result
                        })();
                        return match outcome {
                            Ok(()) => {
                                reply(w, id, Ok(json!({})));
                                (true, false)
                            }
                            Err(message) => {
                                reply(w, id, Err((7, message)));
                                (true, false)
                            }
                        };
                    }
                    let program = if s.cli == AgentCli::ClaudeCode {
                        cfg.claude_program.clone()
                    } else {
                        cfg.codex_program.clone().unwrap_or_else(|| "codex".into())
                    };
                    let args: Vec<String> = if s.cli == AgentCli::ClaudeCode {
                        vec!["stop".into(), session.trim_start_matches("claude:").into()]
                    } else {
                        vec![]
                    };
                    match Command::new(program).args(args).output() {
                        Ok(o) if o.status.success() => Ok(json!({})),
                        Ok(o) => Err((7, String::from_utf8_lossy(&o.stderr).into_owned())),
                        Err(e) => Err((7, e.to_string())),
                    }
                }
            }
        }
    };
    reply(w, id, result);
    (true, false)
}
