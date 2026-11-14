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
    os::fd::AsFd,
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

struct StopRequest {
    thread_id: String,
    deadline: std::time::Instant,
    response: std::sync::mpsc::SyncSender<Result<(), String>>,
}
type StopSender = std::sync::mpsc::SyncSender<StopRequest>;

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
    codex_stop_pending: Arc<AtomicBool>,
    codex_wake: Arc<Wake>,
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
        let codex_stop_pending = Arc::new(AtomicBool::new(false));
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
            codex_stop_pending: Arc::clone(&codex_stop_pending),
            codex_wake: Arc::clone(&wake),
        }));
        let ac = Arc::clone(&stop);
        let ast = Arc::clone(&state);
        let cfg = config.clone();
        let accept_listener = listener;
        let accept = thread::spawn(move || {
            loop {
                if ac.load(Ordering::Relaxed) {
                    break;
                }
                match wait_fd_borrowed(
                    accept_listener.as_fd(),
                    rustix::event::PollFlags::IN
                        | rustix::event::PollFlags::ERR
                        | rustix::event::PollFlags::HUP,
                    Some(Duration::from_secs(1)),
                ) {
                    None => break,
                    Some(false) => continue,
                    Some(true) => {}
                }
                match accept_listener.accept() {
                    Ok((stream, _)) => {
                        let mut state = ast.lock().unwrap();
                        if state.stopping {
                            let _ = stream.shutdown(Shutdown::Both);
                            break;
                        }
                        let client_state = Arc::clone(&ast);
                        let c = cfg.clone();
                        let handle = thread::spawn(move || client(stream, client_state, c));
                        register_thread(&mut state, handle);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
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
            Arc::clone(&codex_stop_pending),
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
        let terminals = {
            let mut state = self.state.lock().unwrap();
            let threads = std::mem::take(&mut state.threads);
            for t in state.terminals.values() {
                kill_pty(&t.pty);
            }
            let terminals = std::mem::take(&mut state.terminals);
            drop(state);
            for handle in threads {
                let _ = handle.join();
            }
            terminals
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
    stops: std::sync::mpsc::Receiver<StopRequest>,
    stop_pending: Arc<AtomicBool>,
) -> Option<thread::JoinHandle<()>> {
    let program = config.codex_program.clone()?;
    Some(thread::spawn(move || {
        let deadline = Arc::new(Mutex::new(None::<std::time::Instant>));
        let pid = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let worker_done = Arc::new(AtomicBool::new(false));
        let ws = Arc::clone(&stop);
        let ww = Arc::clone(&wake);
        let wd = Arc::clone(&deadline);
        let wp = Arc::clone(&pid);
        let wd_done = Arc::clone(&worker_done);
        let watcher = thread::spawn(move || {
            loop {
                let guard = ww.mutex.lock().unwrap();
                if ws.load(Ordering::Relaxed) {
                    let raw = wp.load(Ordering::Relaxed) as i32;
                    if let Some(pid) = rustix::process::Pid::from_raw(raw) {
                        let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
                    }
                }
                if wd_done.load(Ordering::Acquire) {
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
                    interruptible_codex_sleep(&stop, &wake, &stop_pending, Duration::from_secs(30));
                    continue;
                }
            };
            pid.store(child.id(), Ordering::Relaxed);
            if stop.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                pid.store(0, Ordering::Relaxed);
                break;
            }
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
                interruptible_codex_sleep(&stop, &wake, &stop_pending, Duration::from_secs(30));
                continue;
            }
            let mut failed = false;
            while !stop.load(Ordering::Relaxed) {
                loop {
                    let pending = take_codex_stops(&wake, &stop_pending, &stops);
                    if pending.is_empty() {
                        break;
                    }
                    for request in pending {
                        if request.deadline <= std::time::Instant::now() {
                            continue;
                        }
                        set_deadline(
                            &wake,
                            &deadline,
                            Some(std::time::Instant::now() + Duration::from_secs(10)),
                        );
                        let result = client
                            .interrupt(&request.thread_id)
                            .map(|_| ())
                            .map_err(|error| format!("{error:?}"));
                        set_deadline(&wake, &deadline, None);
                        let failed_interrupt = result.is_err();
                        let _ = request.response.send(result);
                        if failed_interrupt {
                            failed = true;
                            break;
                        }
                    }
                    if failed {
                        break;
                    }
                }
                if failed {
                    break;
                }
                set_deadline(
                    &wake,
                    &deadline,
                    Some(std::time::Instant::now() + Duration::from_secs(10)),
                );
                let result = client.loaded_threads();
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
                        interruptible_codex_sleep(&stop, &wake, &stop_pending, delay);
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
                interruptible_codex_sleep(&stop, &wake, &stop_pending, Duration::from_secs(30));
            }
        }
        worker_done.store(true, Ordering::Release);
        wake.notify();
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
fn take_codex_stops(
    wake: &Wake,
    pending: &AtomicBool,
    receiver: &std::sync::mpsc::Receiver<StopRequest>,
) -> Vec<StopRequest> {
    let _guard = wake.mutex.lock().unwrap();
    let requests = receiver.try_iter().collect();
    pending.store(false, Ordering::Release);
    requests
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
fn interruptible_codex_sleep(
    stop: &AtomicBool,
    wake: &Wake,
    stop_pending: &AtomicBool,
    duration: Duration,
) {
    let guard = wake.mutex.lock().unwrap();
    if !stop.load(Ordering::Relaxed) && !stop_pending.load(Ordering::Acquire) {
        let _ = wake
            .condition
            .wait_timeout_while(guard, duration, |_| {
                !stop.load(Ordering::Relaxed) && !stop_pending.load(Ordering::Acquire)
            })
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
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let (err_tx, err_rx) = std::sync::mpsc::channel();
    let _out = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        let _ = out_tx.send(bytes);
    });
    let _err = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        let _ = err_tx.send(bytes);
    });
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "command timed out",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    let stdout = out_rx.recv_timeout(remaining).unwrap_or_default();
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    let stderr = err_rx.recv_timeout(remaining).unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn register_thread(state: &mut State, handle: thread::JoinHandle<()>) {
    let mut index = 0;
    while index < state.threads.len() {
        if state.threads[index].is_finished() {
            let finished = state.threads.swap_remove(index);
            let _ = finished.join();
        } else {
            index += 1;
        }
    }
    state.threads.push(handle);
}

fn kill_pty(pty: &Arc<Mutex<tty::Pty>>) {
    let mut pty = pty.lock().unwrap();
    if matches!(pty.next_child_event(), Some(ChildEvent::Exited(_))) {
        return;
    }
    if let Some(pid) = rustix::process::Pid::from_raw(pty.child().id() as i32) {
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
    }
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
    register_thread(&mut state_guard, writer_thread);
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
    terminal: Arc<Terminal>,
    state: Arc<Mutex<State>>,
    stopping: Arc<AtomicBool>,
) {
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
    let mut processor = Processor::<StdSyncHandler>::new();
    let mut buffer = [0u8; 8192];
    loop {
        match wait_fd(
            &reader,
            rustix::event::PollFlags::IN
                | rustix::event::PollFlags::ERR
                | rustix::event::PollFlags::HUP,
            Some(Duration::from_secs(1)),
        ) {
            Some(true) => {}
            Some(false) => {
                if stopping.load(Ordering::Relaxed) {
                    break;
                }
                continue;
            }
            None => break,
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
    for _ in 0..20 {
        if let Some(ChildEvent::Exited(status)) = terminal.pty.lock().unwrap().next_child_event() {
            code = status.and_then(|status| status.code());
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let mut state = state.lock().unwrap();
    let is_current = state
        .terminals
        .get(&session)
        .is_some_and(|current| Arc::ptr_eq(current, &terminal));
    let clients = {
        let mut terminal_inner = terminal.inner.lock().unwrap();
        terminal_inner.exited = true;
        terminal_inner.clients.clone()
    };
    if is_current {
        state.terminals.remove(&session);
    }
    drop(state);
    kill_pty(&terminal.pty);
    let message = encode_line(&Message::Notification(DaemonNotification::TerminalExited {
        session: session.clone(),
        code,
    }));
    for client in clients {
        let _ = client.enqueue(message.clone());
    }
    drop(inner);
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
                    if wait_fd(
                        &writer,
                        rustix::event::PollFlags::OUT
                            | rustix::event::PollFlags::ERR
                            | rustix::event::PollFlags::HUP,
                        Some(Duration::from_millis(500)),
                    )
                    .is_none()
                    {
                        return;
                    }
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
) -> Option<bool> {
    wait_fd_borrowed(file.as_fd(), events, timeout)
}
fn wait_fd_borrowed(
    fd: std::os::fd::BorrowedFd<'_>,
    events: rustix::event::PollFlags,
    timeout: Option<Duration>,
) -> Option<bool> {
    let mut descriptors = [rustix::event::PollFd::new(&fd, events)];
    let timespec = timeout.map(|duration| rustix::event::Timespec {
        tv_sec: duration.as_secs() as i64,
        tv_nsec: duration.subsec_nanos() as _,
    });
    loop {
        match rustix::event::poll(&mut descriptors, timespec.as_ref()) {
            Ok(ready) => return Some(ready > 0),
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return None,
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
            let Some(s) = state
                .lock()
                .unwrap()
                .sessions
                .iter()
                .find(|s| s.id == session)
                .cloned()
            else {
                reply(w, id, err(3, "unknown session"));
                return (true, false);
            };
            if s.attach.is_none() {
                reply(w, id, err(4, "session cannot be attached"));
                return (true, false);
            }
            let gate = state.lock().unwrap().open_gate.clone();
            let _open_guard = gate.lock().unwrap();
            let mut state_guard = state.lock().unwrap();
            if state_guard.stopping {
                reply(w, id, err(7, "daemon is stopping"));
                return (true, false);
            }
            let mut stale = None;
            let existing = state_guard.terminals.get(&session).cloned();
            let terminal = if let Some(existing) = existing {
                let mut inner = existing.inner.lock().unwrap();
                if inner.exited {
                    state_guard.terminals.remove(&session);
                    stale = Some(existing.clone());
                    None
                } else {
                    existing.pty.lock().unwrap().on_resize(WindowSize {
                        num_lines: rows,
                        num_cols: cols,
                        cell_width: 0,
                        cell_height: 0,
                    });
                    inner
                        .term
                        .resize(TermSize::new(cols as usize, rows as usize));
                    inner.size = (cols, rows);
                    drop(inner);
                    Some(existing)
                }
            } else {
                None
            };
            let mut new_terminal = None;
            let terminal = if let Some(terminal) = terminal {
                terminal
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
                let pty = match tty::new(
                    &options,
                    WindowSize {
                        num_lines: rows,
                        num_cols: cols,
                        cell_width: 0,
                        cell_height: 0,
                    },
                    0,
                ) {
                    Ok(pty) => Arc::new(Mutex::new(pty)),
                    Err(error) => {
                        drop(state_guard);
                        if let Some(stale) = stale {
                            kill_pty(&stale.pty);
                            drop(stale);
                        }
                        reply(w, id, err(7, &error.to_string()));
                        return (true, false);
                    }
                };
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
                let input_file_result = { pty.lock().unwrap().file().try_clone() };
                let input_file = match input_file_result {
                    Ok(file) => file,
                    Err(error) => {
                        kill_pty(&pty);
                        drop(state_guard);
                        drop(pty);
                        if let Some(stale) = stale {
                            kill_pty(&stale.pty);
                            drop(stale);
                        }
                        reply(w, id, err(7, &error.to_string()));
                        return (true, false);
                    }
                };
                let reader_file_result = { pty.lock().unwrap().file().try_clone() };
                let reader_file = match reader_file_result {
                    Ok(file) => file,
                    Err(error) => {
                        kill_pty(&pty);
                        drop(state_guard);
                        drop(pty);
                        if let Some(stale) = stale {
                            kill_pty(&stale.pty);
                            drop(stale);
                        }
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
                state_guard
                    .terminals
                    .insert(session.clone(), Arc::clone(&terminal));
                new_terminal = Some((reader_file, input_file, input_rx, inner));
                terminal
            };
            let mut inner = terminal.inner.lock().unwrap();
            if inner.exited {
                drop(inner);
                state_guard.terminals.remove(&session);
                drop(state_guard);
                kill_pty(&terminal.pty);
                drop(terminal);
                if let Some(stale) = stale {
                    kill_pty(&stale.pty);
                    drop(stale);
                }
                reply(w, id, err(7, "terminal exited while opening"));
                return (true, false);
            }
            if !inner.clients.iter().any(|client| Arc::ptr_eq(client, w)) {
                inner.clients.push(Arc::clone(w));
            }
            opened.insert(session.clone());
            let snapshot = snapshot_ansi(&inner.term);
            let _ = w.enqueue(encode_line(&Message::Response {
                id,
                result: Ok(json!({})),
            }));
            let _ = w.enqueue(encode_line(&Message::Notification(
                DaemonNotification::TerminalOutput {
                    session: session.clone(),
                    data: snapshot,
                },
            )));
            if let Some((file, input_file, input_rx, inner)) = new_terminal {
                let st = Arc::clone(state);
                let name = session.clone();
                let stopping = Arc::clone(&state_guard.stop_flag);
                let writer_stopping = Arc::clone(&stopping);
                let terminal = Arc::clone(&terminal);
                let reader_thread = thread::spawn(move || {
                    terminal_reader(file, name, inner, terminal, st, stopping)
                });
                register_thread(&mut state_guard, reader_thread);
                let writer_thread =
                    thread::spawn(move || terminal_writer(input_file, input_rx, writer_stopping));
                register_thread(&mut state_guard, writer_thread);
            }
            drop(inner);
            drop(state_guard);
            if let Some(stale) = stale {
                kill_pty(&stale.pty);
                drop(stale);
            }
            return (true, false);
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
                        let (sender, wake, pending) = {
                            let state = state.lock().unwrap();
                            (
                                state.codex_stops.clone(),
                                Arc::clone(&state.codex_wake),
                                Arc::clone(&state.codex_stop_pending),
                            )
                        };
                        let outcome = if let Some(sender) = sender {
                            let (tx, rx) = std::sync::mpsc::sync_channel(1);
                            let deadline = std::time::Instant::now() + Duration::from_secs(10);
                            let queued = {
                                let _guard = wake.mutex.lock().unwrap();
                                let result = sender.try_send(StopRequest {
                                    thread_id: thread_id.to_owned(),
                                    deadline,
                                    response: tx,
                                });
                                if result.is_ok() {
                                    pending.store(true, Ordering::Release);
                                    wake.condition.notify_all();
                                }
                                result
                            };
                            queued
                                .map_err(|_| "Codex stop queue is full".to_owned())
                                .and_then(|_| {
                                    rx.recv_timeout(
                                        deadline
                                            .saturating_duration_since(std::time::Instant::now()),
                                    )
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

    #[test]
    fn inherited_pipe_does_not_extend_command_timeout() {
        let started = std::time::Instant::now();
        let pid_file =
            std::env::temp_dir().join(format!("titan-agentd-timeout-child-{}", std::process::id()));
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "sleep 60 & echo $! > \"$1\"; exit 0",
            "sh",
            pid_file.to_str().unwrap(),
        ]);
        let output = run_with_timeout(&mut command, Duration::from_millis(200)).unwrap();
        assert!(output.status.success());
        assert!(started.elapsed() < Duration::from_secs(2));
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
        let _ = std::fs::remove_file(pid_file);
    }
}
