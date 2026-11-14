use crate::{config::Config, discovery::CodexClient, snapshot::snapshot_ansi};
use alacritty_terminal::{
    event::{OnResize, VoidListener, WindowSize},
    term::{Term, test::TermSize},
    tty::{self, ChildEvent, EventedPty, Options, Shell},
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    net::Shutdown,
    os::unix::{
        fs::{DirBuilderExt, MetadataExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{
        Arc, Condvar, Mutex,
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

type StopSender =
    std::sync::mpsc::SyncSender<(String, std::sync::mpsc::SyncSender<Result<(), String>>)>;

struct State {
    sessions: Vec<titan_agent_core::protocol::Session>,
    claude_sessions: Vec<titan_agent_core::protocol::Session>,
    codex_sessions: Vec<titan_agent_core::protocol::Session>,
    subscribers: Vec<Arc<ClientSink>>,
    clients: Vec<Arc<ClientSink>>,
    threads: Vec<thread::JoinHandle<()>>,
    terminals: HashMap<String, Arc<Terminal>>,
    stopping: bool,
    codex_stops: Option<StopSender>,
    open_gate: Arc<Mutex<()>>,
    stop_flag: Arc<AtomicBool>,
}
struct Terminal {
    pty: Arc<Mutex<tty::Pty>>,
    inner: Arc<Mutex<TerminalInner>>,
    input: std::sync::mpsc::SyncSender<Vec<u8>>,
}
struct TerminalInner {
    term: Term<VoidListener>,
    size: (u16, u16),
    clients: Vec<Arc<ClientSink>>,
    exited: bool,
}
struct ClientSink {
    sender: std::sync::mpsc::SyncSender<String>,
    socket: UnixStream,
}
impl ClientSink {
    fn enqueue(&self, line: String) -> bool {
        if self.sender.try_send(line).is_ok() {
            true
        } else {
            let _ = self.socket.shutdown(Shutdown::Both);
            false
        }
    }
}
pub struct Daemon;
struct Wake {
    mutex: Mutex<()>,
    condition: Condvar,
}
impl Wake {
    fn new() -> Self {
        Self {
            mutex: Mutex::new(()),
            condition: Condvar::new(),
        }
    }
    fn notify(&self) {
        let _guard = self.mutex.lock().unwrap();
        self.condition.notify_all();
    }
}
pub struct DaemonHandle {
    socket: PathBuf,
    stop: Arc<AtomicBool>,
    wake: Arc<Wake>,
    state: Arc<Mutex<State>>,
    accept: Option<thread::JoinHandle<()>>,
    poll: Option<thread::JoinHandle<()>>,
    codex: Option<thread::JoinHandle<()>>,
    _lock_file: File,
    socket_identity: (u64, u64),
}
impl Daemon {
    #[allow(clippy::collapsible_if)]
    pub fn start(config: Config) -> std::io::Result<DaemonHandle> {
        let parent = config.socket.parent().unwrap_or(Path::new("."));
        let mut b = fs::DirBuilder::new();
        b.recursive(true).mode(0o700);
        b.create(parent)?;
        let mut lock_path = config.socket.as_os_str().to_os_string();
        lock_path.push(".lock");
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(PathBuf::from(lock_path))?;
        match lock_file.try_lock() {
            Ok(()) => (),
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    "daemon socket is locked",
                ));
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
        if config.socket.exists() {
            if UnixStream::connect(&config.socket).is_ok() {
                return Err(std::io::Error::from(std::io::ErrorKind::AddrInUse));
            }
            let _ = fs::remove_file(&config.socket);
        }
        let listener = UnixListener::bind(&config.socket)?;
        let socket_stat = fs::metadata(&config.socket)?;
        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(Wake::new());
        let state = Arc::new(Mutex::new(State {
            sessions: vec![],
            claude_sessions: vec![],
            codex_sessions: vec![],
            subscribers: vec![],
            clients: vec![],
            threads: Vec::new(),
            terminals: HashMap::new(),
            stopping: false,
            codex_stops: None,
            open_gate: Arc::new(Mutex::new(())),
            stop_flag: Arc::clone(&stop),
        }));
        let ac = Arc::clone(&stop);
        let ast = Arc::clone(&state);
        let cfg = config.clone();
        let accept = thread::spawn(move || {
            loop {
                match listener.accept() {
                    Ok((stream, _)) if ac.load(Ordering::Relaxed) => {
                        let _ = stream.shutdown(Shutdown::Both);
                        break;
                    }
                    Ok((stream, _)) => {
                        let s = Arc::clone(&ast);
                        let client_state = Arc::clone(&s);
                        let c = cfg.clone();
                        let handle = thread::spawn(move || client(stream, client_state, c));
                        s.lock().unwrap().threads.push(handle);
                    }
                    Err(_) => break,
                }
            }
        });
        let ps = Arc::clone(&stop);
        let pst = Arc::clone(&state);
        let pc = config.clone();
        let pw = Arc::clone(&wake);
        let poll = thread::spawn(move || {
            let mut old_claude = Vec::new();
            let projects = std::cell::RefCell::new(HashMap::<
                String,
                titan_agent_core::protocol::Project,
            >::new());
            while !ps.load(Ordering::Relaxed) {
                let mut claude_sessions = old_claude.clone();
                let mut command = Command::new(&pc.claude_program);
                command.args(["agents", "--json"]);
                match run_with_timeout(&mut command, Duration::from_secs(5)) {
                    Ok(output) if output.status.success() => match String::from_utf8(output.stdout)
                    {
                        Ok(text) => match sessions_from_claude_agents(&text, &|cwd| {
                            project(cwd, &pc, &mut projects.borrow_mut())
                        }) {
                            Ok(value) => claude_sessions = value,
                            Err(error) => {
                                eprintln!("claude discovery returned invalid JSON: {error}")
                            }
                        },
                        Err(error) => eprintln!("claude discovery returned invalid UTF-8: {error}"),
                    },
                    Ok(output) => eprintln!(
                        "claude discovery failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ),
                    Err(error) => eprintln!("claude discovery failed: {error}"),
                }
                old_claude = claude_sessions.clone();
                publish_half(&pst, true, claude_sessions);
                interruptible_sleep(
                    &ps,
                    &pw,
                    if pst.lock().unwrap().subscribers.is_empty() {
                        pc.poll_idle
                    } else {
                        pc.poll_subscribed
                    },
                );
            }
        });
        let (stop_tx, stop_rx) = std::sync::mpsc::sync_channel(32);
        state.lock().unwrap().codex_stops = Some(stop_tx);
        let codex = spawn_codex_worker(
            Arc::clone(&stop),
            Arc::clone(&wake),
            Arc::clone(&state),
            config.clone(),
            stop_rx,
        );
        Ok(DaemonHandle {
            socket: config.socket,
            stop,
            wake,
            state,
            accept: Some(accept),
            poll: Some(poll),
            codex,
            _lock_file: lock_file,
            socket_identity: (socket_stat.dev(), socket_stat.ino()),
        })
    }
}
impl DaemonHandle {
    pub fn socket(&self) -> &Path {
        &self.socket
    }
    pub fn shutdown(mut self) {
        {
            let mut state = self.state.lock().unwrap();
            state.stopping = true;
            for client in &state.clients {
                let _ = client.socket.shutdown(Shutdown::Both);
            }
            state.clients.clear();
            state.subscribers.clear();
        }
        self.stop.store(true, Ordering::Relaxed);
        self.wake.notify();
        let _ = UnixStream::connect(&self.socket);
        let terminals = {
            let mut state = self.state.lock().unwrap();
            for t in state.terminals.values() {
                if let Some(pid) =
                    rustix::process::Pid::from_raw(t.pty.lock().unwrap().child().id() as i32)
                {
                    let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
                }
            }
            std::mem::take(&mut state.terminals)
        };
        drop(terminals);
        if let Some(h) = self.accept.take() {
            let _ = h.join();
        }
        if let Some(h) = self.poll.take() {
            let _ = h.join();
        }
        if let Some(h) = self.codex.take() {
            let _ = h.join();
        }
        let threads = {
            let mut state = self.state.lock().unwrap();
            state.terminals.clear();
            std::mem::take(&mut state.threads)
        };
        for handle in threads {
            let _ = handle.join();
        }
        if fs::metadata(&self.socket).is_ok_and(|m| (m.dev(), m.ino()) == self.socket_identity) {
            let _ = fs::remove_file(&self.socket);
        }
    }
}

fn publish_half(
    state: &Arc<Mutex<State>>,
    claude: bool,
    half: Vec<titan_agent_core::protocol::Session>,
) {
    let mut state = state.lock().unwrap();
    if claude {
        state.claude_sessions = half
    } else {
        state.codex_sessions = half
    }
    let mut combined = state.claude_sessions.clone();
    combined.extend(state.codex_sessions.clone());
    if combined == state.sessions {
        return;
    }
    state.sessions = combined.clone();
    let line = encode_line(&Message::Notification(
        DaemonNotification::SessionsChanged { sessions: combined },
    ));
    for stream in &mut state.subscribers {
        let _ = stream.enqueue(line.clone());
    }
}

#[allow(clippy::collapsible_if)]
fn spawn_codex_worker(
    stop: Arc<AtomicBool>,
    wake: Arc<Wake>,
    state: Arc<Mutex<State>>,
    config: Config,
    stops: std::sync::mpsc::Receiver<(String, std::sync::mpsc::SyncSender<Result<(), String>>)>,
) -> Option<thread::JoinHandle<()>> {
    let program = config.codex_program.clone()?;
    Some(thread::spawn(move || {
        let deadline = Arc::new(Mutex::new(None::<std::time::Instant>));
        let pid = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let ws = Arc::clone(&stop);
        let ww = Arc::clone(&wake);
        let wd = Arc::clone(&deadline);
        let wp = Arc::clone(&pid);
        let watcher = thread::spawn(move || {
            loop {
                let guard = ww.mutex.lock().unwrap();
                if ws.load(Ordering::Relaxed) {
                    let raw = wp.load(Ordering::Relaxed) as i32;
                    if let Some(pid) = rustix::process::Pid::from_raw(raw) {
                        let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
                    }
                    break;
                }
                let deadline_value = *wd.lock().unwrap();
                if deadline_value.is_some_and(|until| std::time::Instant::now() >= until) {
                    let raw = wp.load(Ordering::Relaxed) as i32;
                    if let Some(pid) = rustix::process::Pid::from_raw(raw) {
                        let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
                    }
                    *wd.lock().unwrap() = None;
                    drop(guard);
                    continue;
                }
                match deadline_value {
                    Some(until) => {
                        let duration = until.saturating_duration_since(std::time::Instant::now());
                        let _ = ww.condition.wait_timeout(guard, duration).unwrap();
                    }
                    None => {
                        drop(ww.condition.wait(guard).unwrap());
                    }
                }
            }
        });
        let projects =
            std::cell::RefCell::new(HashMap::<String, titan_agent_core::protocol::Project>::new());
        while !stop.load(Ordering::Relaxed) {
            let mut child = match Command::new(&program)
                .args(["app-server", "proxy"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => child,
                Err(error) => {
                    eprintln!("Codex app-server failed to start: {error}");
                    publish_half(&state, false, Vec::new());
                    interruptible_sleep(&stop, &wake, Duration::from_secs(30));
                    continue;
                }
            };
            pid.store(child.id(), Ordering::Relaxed);
            let Some(input) = child.stdin.take() else {
                let _ = child.kill();
                let _ = child.wait();
                pid.store(0, Ordering::Relaxed);
                continue;
            };
            let Some(output) = child.stdout.take() else {
                let _ = child.kill();
                let _ = child.wait();
                pid.store(0, Ordering::Relaxed);
                continue;
            };
            let mut client = CodexClient::new(BufReader::new(output), input);
            set_deadline(
                &wake,
                &deadline,
                Some(std::time::Instant::now() + Duration::from_secs(10)),
            );
            let initialized = client.initialize(env!("CARGO_PKG_VERSION"));
            set_deadline(&wake, &deadline, None);
            if initialized.is_err() {
                let _ = child.kill();
                let _ = child.wait();
                pid.store(0, Ordering::Relaxed);
                publish_half(&state, false, Vec::new());
                interruptible_sleep(&stop, &wake, Duration::from_secs(30));
                continue;
            }
            let mut failed = false;
            while !stop.load(Ordering::Relaxed) {
                set_deadline(
                    &wake,
                    &deadline,
                    Some(std::time::Instant::now() + Duration::from_secs(10)),
                );
                let result = if let Ok((thread_id, response)) = stops.try_recv() {
                    set_deadline(
                        &wake,
                        &deadline,
                        Some(std::time::Instant::now() + Duration::from_secs(10)),
                    );
                    let result = client
                        .interrupt(&thread_id)
                        .map(|_| ())
                        .map_err(|error| format!("{error:?}"));
                    set_deadline(&wake, &deadline, None);
                    let _ = response.send(result);
                    set_deadline(
                        &wake,
                        &deadline,
                        Some(std::time::Instant::now() + Duration::from_secs(10)),
                    );
                    client.loaded_threads()
                } else {
                    client.loaded_threads()
                };
                set_deadline(&wake, &deadline, None);
                match result {
                    Ok(threads) => {
                        let sessions = threads
                            .iter()
                            .filter_map(|t| {
                                session_from_codex_thread(t, &|cwd| {
                                    project(cwd, &config, &mut projects.borrow_mut())
                                })
                            })
                            .collect();
                        publish_half(&state, false, sessions);
                        let delay = if state.lock().unwrap().subscribers.is_empty() {
                            config.poll_idle
                        } else {
                            config.poll_subscribed
                        };
                        interruptible_sleep(&stop, &wake, delay);
                    }
                    Err(error) => {
                        eprintln!("Codex app-server request failed: {error:?}");
                        failed = true;
                        break;
                    }
                }
            }
            if stop.load(Ordering::Relaxed) {
                let _ = child.kill();
            }
            if failed || stop.load(Ordering::Relaxed) {
                let _ = child.kill();
            }
            let _ = child.wait();
            pid.store(0, Ordering::Relaxed);
            if failed {
                publish_half(&state, false, Vec::new());
                interruptible_sleep(&stop, &wake, Duration::from_secs(30));
            }
        }
        let _ = watcher.join();
    }))
}
fn set_deadline(
    wake: &Wake,
    deadline: &Mutex<Option<std::time::Instant>>,
    value: Option<std::time::Instant>,
) {
    let _guard = wake.mutex.lock().unwrap();
    *deadline.lock().unwrap() = value;
    wake.condition.notify_all();
}
fn interruptible_sleep(stop: &AtomicBool, wake: &Wake, duration: Duration) {
    let guard = wake.mutex.lock().unwrap();
    if !stop.load(Ordering::Relaxed) {
        let _ = wake
            .condition
            .wait_timeout_while(guard, duration, |_| !stop.load(Ordering::Relaxed))
            .unwrap();
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
            let mut command = Command::new(g);
            command.args([
                "-C",
                cwd,
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
            ]);
            run_with_timeout(&mut command, Duration::from_secs(5)).ok()
        })
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| project_from_git_common_dir(s.trim()))
        .unwrap_or_else(|| fallback_project(cwd, &has));
    cache.insert(cwd.into(), p.clone());
    p
}

fn run_with_timeout(command: &mut Command, timeout: Duration) -> std::io::Result<Output> {
    use std::io::Read;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let err = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out.join();
            let _ = err.join();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "command timed out",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };
    Ok(Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

fn client(stream: UnixStream, state: Arc<Mutex<State>>, config: Config) {
    let reader = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let writer_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let (sender, receiver) = std::sync::mpsc::sync_channel::<String>(1024);
    let writer = Arc::new(ClientSink {
        sender,
        socket: stream.try_clone().unwrap(),
    });
    let mut state_guard = state.lock().unwrap();
    if state_guard.stopping {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    state_guard.clients.push(Arc::clone(&writer));
    let writer_thread = thread::spawn(move || {
        let mut output = writer_stream;
        while let Ok(line) = receiver.recv() {
            if output.write_all(line.as_bytes()).is_err() {
                let _ = output.shutdown(Shutdown::Both);
                break;
            }
        }
    });
    state_guard.threads.push(writer_thread);
    drop(state_guard);
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
            .inner
            .lock()
            .unwrap()
            .clients
            .retain(|s| !Arc::ptr_eq(s, &writer));
    }
}
fn reply(w: &Arc<ClientSink>, id: u64, result: Result<Value, (i32, String)>) {
    let r = result.map_err(|(code, message)| RpcError { code, message });
    let _ = w.enqueue(encode_line(&Message::Response { id, result: r }));
}
fn terminal_for(
    state: &Arc<Mutex<State>>,
    session: &str,
    writer: &Arc<ClientSink>,
) -> Option<Arc<Terminal>> {
    let state = state.lock().unwrap();
    let terminal = state.terminals.get(session)?.clone();
    let inner = terminal.inner.lock().unwrap();
    let attached = !inner.exited && inner.clients.iter().any(|c| Arc::ptr_eq(c, writer));
    drop(inner);
    if attached { Some(terminal) } else { None }
}
fn terminal_reader(
    mut reader: std::fs::File,
    session: String,
    inner: Arc<Mutex<TerminalInner>>,
    pty: std::sync::Weak<Mutex<tty::Pty>>,
    terminal: std::sync::Weak<Terminal>,
    state: Arc<Mutex<State>>,
) {
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
    let mut processor = Processor::<StdSyncHandler>::new();
    let mut buffer = [0u8; 8192];
    loop {
        if !wait_fd(
            &reader,
            rustix::event::PollFlags::IN
                | rustix::event::PollFlags::ERR
                | rustix::event::PollFlags::HUP,
            None,
        ) {
            break;
        }
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(_) => break,
            Ok(n) => {
                let mut inner = inner.lock().unwrap();
                processor.advance(&mut inner.term, &buffer[..n]);
                let message =
                    encode_line(&Message::Notification(DaemonNotification::TerminalOutput {
                        session: session.clone(),
                        data: buffer[..n].to_vec(),
                    }));
                inner
                    .clients
                    .retain(|client| client.enqueue(message.clone()));
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
    let mut state = state.lock().unwrap();
    if let (Some(current), Some(old)) = (state.terminals.get(&session), terminal.upgrade())
        && Arc::ptr_eq(current, &old)
    {
        state.terminals.remove(&session);
    }
    drop(state);
    if let Some(terminal) = terminal.upgrade() {
        terminal.inner.lock().unwrap().exited = true;
    }
    let clients = inner.lock().unwrap().clients.clone();
    let message = encode_line(&Message::Notification(DaemonNotification::TerminalExited {
        session: session.clone(),
        code,
    }));
    for client in clients {
        let _ = client.enqueue(message.clone());
    }
}
fn terminal_writer(
    mut writer: std::fs::File,
    receiver: std::sync::mpsc::Receiver<Vec<u8>>,
    stopping: Arc<AtomicBool>,
) {
    while !stopping.load(Ordering::Relaxed) {
        let bytes = match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(bytes) => bytes,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let mut offset = 0;
        while offset < bytes.len() && !stopping.load(Ordering::Relaxed) {
            match writer.write(&bytes[offset..]) {
                Ok(0) => return,
                Ok(count) => offset += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    let _ = wait_fd(
                        &writer,
                        rustix::event::PollFlags::OUT
                            | rustix::event::PollFlags::ERR
                            | rustix::event::PollFlags::HUP,
                        Some(Duration::from_millis(500)),
                    );
                }
                Err(_) => return,
            }
        }
    }
}
fn wait_fd(
    file: &std::fs::File,
    events: rustix::event::PollFlags,
    timeout: Option<Duration>,
) -> bool {
    let mut descriptors = [rustix::event::PollFd::new(file, events)];
    let timespec = timeout.map(|duration| rustix::event::Timespec {
        tv_sec: duration.as_secs() as i64,
        tv_nsec: duration.subsec_nanos() as _,
    });
    loop {
        match rustix::event::poll(&mut descriptors, timespec.as_ref()) {
            Ok(ready) => return ready > 0,
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return false,
        }
    }
}
fn handle(
    w: &Arc<ClientSink>,
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
            let mut state = state.lock().unwrap();
            if state.stopping {
                reply(w, id, err(7, "daemon is stopping"));
                return (true, false);
            }
            if !state
                .subscribers
                .iter()
                .any(|client| Arc::ptr_eq(client, w))
            {
                state.subscribers.push(Arc::clone(w));
            }
            let sessions = state.sessions.clone();
            let _ = w.enqueue(encode_line(&Message::Response {
                id,
                result: Ok(json!({"sessions":sessions})),
            }));
            return (true, false);
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
                    let gate = state.lock().unwrap().open_gate.clone();
                    let _open_guard = gate.lock().unwrap();
                    if state.lock().unwrap().stopping {
                        reply(w, id, err(7, "daemon is stopping"));
                        return (true, false);
                    }
                    let mut reader_file = None;
                    let existing = state
                        .lock()
                        .unwrap()
                        .terminals
                        .get(&session)
                        .filter(|t| !t.inner.lock().unwrap().exited)
                        .cloned();
                    let terminal = if let Some(t) = existing {
                        let mut inner = t.inner.lock().unwrap();
                        let mut pty = t.pty.lock().unwrap();
                        pty.on_resize(WindowSize {
                            num_lines: rows,
                            num_cols: cols,
                            cell_width: 0,
                            cell_height: 0,
                        });
                        inner
                            .term
                            .resize(TermSize::new(cols as usize, rows as usize));
                        inner.size = (cols, rows);
                        drop(pty);
                        drop(inner);
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
                                let inner = Arc::new(Mutex::new(TerminalInner {
                                    term: Term::new(
                                        Default::default(),
                                        &TermSize::new(cols as usize, rows as usize),
                                        VoidListener,
                                    ),
                                    size: (cols, rows),
                                    clients: Vec::new(),
                                    exited: false,
                                }));
                                let pty = Arc::new(Mutex::new(pty));
                                let input_file = match pty.lock().unwrap().file().try_clone() {
                                    Ok(file) => file,
                                    Err(error) => {
                                        reply(w, id, err(7, &error.to_string()));
                                        return (true, false);
                                    }
                                };
                                let (input_tx, input_rx) = std::sync::mpsc::sync_channel(1024);
                                let terminal = Arc::new(Terminal {
                                    pty: Arc::clone(&pty),
                                    inner: Arc::clone(&inner),
                                    input: input_tx,
                                });
                                let file = match terminal.pty.lock().unwrap().file().try_clone() {
                                    Ok(file) => file,
                                    Err(error) => {
                                        reply(w, id, err(7, &error.to_string()));
                                        return (true, false);
                                    }
                                };
                                let mut state_guard = state.lock().unwrap();
                                if state_guard.stopping {
                                    reply(w, id, err(7, "daemon is stopping"));
                                    return (true, false);
                                }
                                state_guard
                                    .terminals
                                    .insert(session.clone(), Arc::clone(&terminal));
                                drop(state_guard);
                                reader_file = Some((
                                    file,
                                    input_file,
                                    input_rx,
                                    inner,
                                    Arc::downgrade(&pty),
                                    Arc::downgrade(&terminal),
                                ));
                                terminal
                            }
                            Err(e) => {
                                reply(w, id, err(7, &e.to_string()));
                                return (true, false);
                            }
                        }
                    };
                    let mut inner = terminal.inner.lock().unwrap();
                    if inner.exited {
                        return (true, false);
                    }
                    if !inner.clients.iter().any(|c| Arc::ptr_eq(c, w)) {
                        inner.clients.push(Arc::clone(w));
                    }
                    opened.insert(session.clone());
                    let snap = snapshot_ansi(&inner.term);
                    let _ = w.enqueue(encode_line(&Message::Response {
                        id,
                        result: Ok(json!({})),
                    }));
                    let _ = w.enqueue(encode_line(&Message::Notification(
                        DaemonNotification::TerminalOutput {
                            session: session.clone(),
                            data: snap,
                        },
                    )));
                    drop(inner);
                    if let Some((file, input_file, input_rx, inner, pty_ref, terminal_ref)) =
                        reader_file
                    {
                        let st = Arc::clone(state);
                        let name = session.clone();
                        let stopping = state.lock().unwrap().stop_flag.clone();
                        let reader_thread = thread::spawn(move || {
                            terminal_reader(file, name, inner, pty_ref, terminal_ref, st)
                        });
                        let writer_thread =
                            thread::spawn(move || terminal_writer(input_file, input_rx, stopping));
                        state
                            .lock()
                            .unwrap()
                            .threads
                            .extend([reader_thread, writer_thread]);
                    }
                    return (true, false);
                }
            }
        }
        ClientRequest::TerminalInput { session, data } => terminal_for(state, &session, w)
            .map_or_else(
                || err(3, "terminal not attached"),
                |t| {
                    t.input
                        .try_send(data)
                        .map(|_| json!({}))
                        .map_err(|_| (7, "terminal input queue is full".into()))
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
                    let mut inner = t.inner.lock().unwrap();
                    t.pty.lock().unwrap().on_resize(WindowSize {
                        num_lines: rows,
                        num_cols: cols,
                        cell_width: 0,
                        cell_height: 0,
                    });
                    inner
                        .term
                        .resize(TermSize::new(cols as usize, rows as usize));
                    inner.size = (cols, rows);
                    Ok(json!({}))
                },
            )
        }
        ClientRequest::TerminalClose { session } => terminal_for(state, &session, w).map_or_else(
            || err(3, "terminal not attached"),
            |t| {
                t.inner
                    .lock()
                    .unwrap()
                    .clients
                    .retain(|c| !Arc::ptr_eq(c, w));
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
                        let Some(thread_id) = session.strip_prefix("codex:") else {
                            return (
                                reply(w, id, err(7, "invalid Codex session id")),
                                (true, false),
                            )
                                .1;
                        };
                        let sender = state.lock().unwrap().codex_stops.clone();
                        let outcome = if let Some(sender) = sender {
                            let (tx, rx) = std::sync::mpsc::sync_channel(1);
                            sender
                                .try_send((thread_id.to_owned(), tx))
                                .map_err(|_| "Codex stop queue is full".to_owned())
                                .and_then(|_| {
                                    rx.recv_timeout(Duration::from_secs(10))
                                        .map_err(|_| "Codex interrupt timed out".to_owned())?
                                })
                        } else {
                            Err("Codex unavailable".to_owned())
                        };
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
                    let mut command = Command::new(program);
                    command.args(args);
                    match run_with_timeout(&mut command, Duration::from_secs(10)) {
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

#[cfg(test)]
mod timeout_tests {
    use super::run_with_timeout;
    use std::{
        process::Command,
        time::{Duration, Instant},
    };

    #[test]
    fn command_timeout_kills_and_reaps_child() {
        let start = Instant::now();
        let error = run_with_timeout(Command::new("sleep").arg("10"), Duration::from_millis(200))
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
