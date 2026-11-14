use crate::{config::Config, discovery::CodexClient, snapshot::snapshot_ansi};
use alacritty_terminal::term::{Term, test::TermSize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufReader, Write},
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
    subscribers: Vec<UnixStream>,
    running: HashSet<String>,
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
            running: HashSet::new(),
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
            let projects = std::cell::RefCell::new(HashMap::<
                String,
                titan_agent_core::protocol::Project,
            >::new());
            while !ps.load(Ordering::Relaxed) {
                let mut sessions = Vec::new();
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
                                sessions = v
                            }
                        }
                    }
                } else {
                    eprintln!("claude discovery failed")
                }
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
                        let _ = stream.write_all(
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
                thread::sleep(pc.poll_subscribed.min(Duration::from_millis(200)));
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
        self.state.lock().unwrap().subscribers.clear();
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
fn client(mut stream: UnixStream, state: Arc<Mutex<State>>, config: Config) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let reader = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut lines = LineReader::new(BufReader::new(reader));
    let mut hello = false;
    let mut opened = HashSet::new();
    loop {
        match lines.next_line() {
            Ok(Some(line)) => match decode_line(&line) {
                Ok(Message::Request { id, request }) => {
                    let (res, close) = handle(
                        &mut stream,
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
                        &mut stream,
                        id,
                        Err((error_code::UNKNOWN_METHOD, "unknown method".into())),
                    );
                }
                Err(DecodeError::InvalidParams {
                    id: Some(id),
                    message,
                }) => reply(&mut stream, id, Err((error_code::INVALID_PARAMS, message))),
                Err(DecodeError::InvalidJson(_) | DecodeError::TooLong) => break,
                Err(_) => {}
            },
            Ok(None) => break,
            Err(_) => break,
        }
    }
    state
        .lock()
        .unwrap()
        .subscribers
        .retain(|s| s.peer_addr().is_ok());
}
fn reply(w: &mut UnixStream, id: u64, result: Result<Value, (i32, String)>) {
    let r = result.map_err(|(code, message)| RpcError { code, message });
    let _ = w.write_all(encode_line(&Message::Response { id, result: r }).as_bytes());
}
fn handle(
    w: &mut UnixStream,
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
            if let Ok(s) = w.try_clone() {
                state.lock().unwrap().subscribers.push(s)
            }
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
                    let mut term = Term::new(
                        Default::default(),
                        &TermSize::new(cols as usize, rows as usize),
                        alacritty_terminal::event::VoidListener,
                    );
                    let snap = snapshot_ansi(&term);
                    let _ = &mut term;
                    opened.insert(session.clone());
                    let mut st = state.lock().unwrap();
                    st.running.insert(session.clone());
                    drop(st);
                    let _ = w.write_all(
                        encode_line(&Message::Response {
                            id,
                            result: Ok(json!({})),
                        })
                        .as_bytes(),
                    );
                    let _ = w.write_all(
                        encode_line(&Message::Notification(DaemonNotification::TerminalOutput {
                            session,
                            data: snap,
                        }))
                        .as_bytes(),
                    );
                    let program = if s.cli == AgentCli::ClaudeCode {
                        cfg.claude_program.clone()
                    } else {
                        cfg.codex_program.clone().unwrap_or_else(|| "codex".into())
                    };
                    let mut args = s.attach.unwrap();
                    if !args.is_empty() {
                        args.remove(0);
                    }
                    let child = Command::new(program)
                        .args(args)
                        .current_dir(s.cwd)
                        .env("TERM", "xterm-256color")
                        .env("COLORTERM", "truecolor")
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn();
                    if let Ok(mut child) = child {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    return (true, false);
                }
            }
        }
        ClientRequest::TerminalInput { .. }
        | ClientRequest::TerminalResize { .. }
        | ClientRequest::TerminalClose { .. } => err(3, "terminal not attached"),
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
                    let program = if s.cli == AgentCli::ClaudeCode {
                        cfg.claude_program.clone()
                    } else {
                        "codex".into()
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
