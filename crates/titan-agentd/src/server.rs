use crate::pty::{PtyError, PtyHandler, PtyProcess, PtySpawn};
use crate::snapshot::Emulator;
use crate::source::{SessionSource, StopError};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Condvar;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use titan_agent_core::paths::lock_path;
use titan_agent_core::protocol::*;
use titan_agent_core::session::{Session, SessionKey};

// Lock order: registry -> terminal state -> terminal process. Never acquire an
// earlier lock while holding a later one. Socket writes, source calls, and
// PtyProcess::kill always happen without these locks held.

pub struct DaemonConfig {
    pub socket: PathBuf,
    pub poll_interval: Duration,
    pub client_queue: usize,
    pub kill_grace: Duration,
}
impl DaemonConfig {
    pub fn new(socket: PathBuf) -> DaemonConfig {
        Self {
            socket,
            poll_interval: Duration::from_secs(3),
            client_queue: 4096,
            kill_grace: Duration::from_secs(2),
        }
    }
}
#[derive(Debug)]
pub enum DaemonError {
    AlreadyRunning,
    Io(std::io::Error),
}
impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => f.write_str("daemon is already running"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for DaemonError {}
impl From<std::io::Error> for DaemonError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Clone)]
pub struct ShutdownHandle {
    flag: Arc<AtomicBool>,
}
impl ShutdownHandle {
    pub fn shutdown(&self) {
        self.flag.store(true, Ordering::Release);
    }
}

struct Client {
    id: u64,
    tx: Mutex<Option<SyncSender<Vec<u8>>>>,
    stream: UnixStream,
    subscribed: AtomicBool,
    writer_done: Arc<AtomicBool>,
}
type Clients = Arc<Mutex<HashMap<u64, Arc<Client>>>>;
type Connections = Arc<Mutex<Vec<Arc<Client>>>>;
struct TermState {
    emulator: Emulator,
    clients: HashSet<u64>,
    exited: bool,
    pending_replies: Vec<Vec<u8>>,
}
struct Terminal {
    id: u64,
    state: Mutex<TermState>,
    process: Mutex<Option<Arc<PtyProcess>>>,
    grace: Duration,
    clients: Clients,
    registry: Arc<Mutex<Registry>>,
    cleanup_workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
}
struct Registry {
    terminals: HashMap<TerminalId, Arc<Terminal>>,
    sessions: HashMap<SessionKey, TerminalId>,
    next: u64,
    closed: bool,
}
struct Shared {
    clients: Clients,
    connections: Connections,
    registry: Arc<Mutex<Registry>>,
    sessions: Mutex<Vec<Session>>,
    source: Mutex<Box<dyn SessionSource>>,
    shutdown: Arc<AtomicBool>,
    discovery: (Mutex<DiscoveryState>, Condvar),
    config: DaemonConfig,
    cleanup_workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
}
struct DiscoveryState {
    subscribers: HashSet<u64>,
    requested: bool,
}
pub struct Daemon {
    shared: Arc<Shared>,
    listener: UnixListener,
    lock: Option<File>,
    socket: PathBuf,
}

impl Daemon {
    pub fn bind(
        config: DaemonConfig,
        source: Box<dyn SessionSource>,
    ) -> Result<Daemon, DaemonError> {
        let parent = config
            .socket
            .parent()
            .ok_or_else(|| std::io::Error::other("socket has no parent"))?;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(parent)?;
        let lock_path = lock_path(&config.socket);
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path)?;
        if lock.try_lock().is_err() {
            return Err(DaemonError::AlreadyRunning);
        }
        if config.socket.exists() {
            fs::remove_file(&config.socket)?;
        }
        let listener = UnixListener::bind(&config.socket)?;
        listener.set_nonblocking(true)?;
        let mut source = source;
        let sessions = normalize(source.poll(), None);
        let socket = config.socket.clone();
        let shared = Arc::new(Shared {
            clients: Arc::new(Mutex::new(HashMap::new())),
            connections: Arc::new(Mutex::new(Vec::new())),
            cleanup_workers: Arc::new(Mutex::new(Vec::new())),
            registry: Arc::new(Mutex::new(Registry {
                terminals: HashMap::new(),
                sessions: HashMap::new(),
                next: 1,
                closed: false,
            })),
            sessions: Mutex::new(sessions),
            source: Mutex::new(source),
            shutdown: Arc::new(AtomicBool::new(false)),
            discovery: (
                Mutex::new(DiscoveryState {
                    subscribers: HashSet::new(),
                    requested: false,
                }),
                Condvar::new(),
            ),
            config,
        });
        Ok(Daemon {
            shared,
            listener,
            lock: Some(lock),
            socket,
        })
    }
    pub fn shutdown_handle(&self) -> ShutdownHandle {
        ShutdownHandle {
            flag: self.shared.shutdown.clone(),
        }
    }
    pub fn run(mut self) -> Result<(), DaemonError> {
        let mut workers = Vec::new();
        let discovery_shared = self.shared.clone();
        let discovery_thread = thread::spawn(move || discovery_loop(discovery_shared));
        while !self.shared.shutdown.load(Ordering::Acquire) {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    let id = NEXT_CLIENT.fetch_add(1, Ordering::Relaxed);
                    let (tx, rx) = mpsc::sync_channel(self.shared.config.client_queue.max(1));
                    let writer_done = Arc::new(AtomicBool::new(false));
                    let writer = match stream.try_clone() {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("titan-agentd: {e}");
                            continue;
                        }
                    };
                    let client = Arc::new(Client {
                        id,
                        tx: Mutex::new(Some(tx)),
                        stream: match stream.try_clone() {
                            Ok(v) => v,
                            Err(e) => {
                                eprintln!("titan-agentd: {e}");
                                continue;
                            }
                        },
                        subscribed: AtomicBool::new(false),
                        writer_done: writer_done.clone(),
                    });
                    self.shared
                        .clients
                        .lock()
                        .unwrap()
                        .insert(id, client.clone());
                    self.shared.connections.lock().unwrap().push(client);
                    let connections = self.shared.connections.clone();
                    workers.push(thread::spawn(move || {
                        writer_loop(writer, rx, writer_done, id, connections)
                    }));
                    let s = self.shared.clone();
                    workers.push(thread::spawn(move || client_loop(stream, id, s)));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(e) => {
                    eprintln!("titan-agentd accept: {e}");
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
        {
            let mut reg = self.shared.registry.lock().unwrap();
            reg.closed = true;
        }
        {
            let (lock, cv) = &self.shared.discovery;
            lock.lock().unwrap().requested = true;
            cv.notify_all();
        }
        let clients = self.shared.connections.lock().unwrap().clone();
        for c in &clients {
            c.tx.lock().unwrap().take();
        }
        self.shared.clients.lock().unwrap().clear();
        let (dlock, dcv) = &self.shared.discovery;
        dlock.lock().unwrap().subscribers.clear();
        dcv.notify_all();
        let drain_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while clients
            .iter()
            .any(|c| !c.writer_done.load(Ordering::Acquire))
            && std::time::Instant::now() < drain_deadline
        {
            thread::sleep(Duration::from_millis(5));
        }
        for c in &clients {
            let _ = c.stream.shutdown(std::net::Shutdown::Both);
        }
        drop(clients);
        let terms: Vec<_> = self
            .shared
            .registry
            .lock()
            .unwrap()
            .terminals
            .values()
            .cloned()
            .collect();
        let mut killers = Vec::new();
        for t in terms {
            killers.push(thread::spawn(move || {
                let process = t.process.lock().unwrap().clone();
                if let Some(p) = process {
                    p.kill(t.grace)
                }
            }));
        }
        for h in killers {
            let _ = h.join();
        }
        for h in workers {
            let _ = h.join();
        }
        for h in self.shared.cleanup_workers.lock().unwrap().drain(..) {
            let _ = h.join();
        }
        let _ = discovery_thread.join();
        let _ = fs::remove_file(&self.socket);
        self.lock.take();
        Ok(())
    }
}
static NEXT_CLIENT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn normalize(mut rows: Vec<Session>, previous: Option<&[Session]>) -> Vec<Session> {
    for row in &mut rows {
        let old = previous.and_then(|p| p.iter().find(|x| x.key == row.key));
        row.state_since_ms = match old {
            Some(x) if x.state == row.state => x.state_since_ms,
            Some(_) => Some(unix_ms()),
            None => row.state_since_ms.or_else(|| Some(unix_ms())),
        };
    }
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    rows
}
fn send(client: &Client, msg: ServerMessage) -> bool {
    let result = client
        .tx
        .lock()
        .unwrap()
        .as_ref()
        .map(|tx| tx.try_send(encode_line(&msg)));
    match result {
        Some(Ok(())) => true,
        Some(Err(TrySendError::Full(_) | TrySendError::Disconnected(_))) | None => {
            let _ = client.stream.shutdown(std::net::Shutdown::Both);
            false
        }
    }
}
fn reply(id: u64, r: Response) -> ServerMessage {
    ServerMessage::Response { id, response: r }
}
fn error(id: u64, c: ErrorCode, m: &str) -> ServerMessage {
    ServerMessage::Error {
        id,
        error: ErrorBody {
            code: c,
            message: m.into(),
        },
    }
}
fn writer_loop(
    mut stream: UnixStream,
    rx: mpsc::Receiver<Vec<u8>>,
    done: Arc<AtomicBool>,
    id: u64,
    connections: Connections,
) {
    for bytes in rx {
        if stream.write_all(&bytes).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
    done.store(true, Ordering::Release);
    connections.lock().unwrap().retain(|client| client.id != id);
}
fn client_loop(stream: UnixStream, id: u64, shared: Arc<Shared>) {
    let mut reader = BufReader::new(stream);
    let mut greeted = false;
    loop {
        if shared.shutdown.load(Ordering::Acquire) {
            break;
        }
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match reader.read(&mut byte) {
                Ok(0) => {
                    cleanup_client(id, &shared);
                    return;
                }
                Ok(_) => {
                    if byte[0] == b'\n' {
                        break;
                    }
                    line.push(byte[0]);
                    if line.len() > MAX_FRAME_BYTES {
                        cleanup_client(id, &shared);
                        return;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    cleanup_client(id, &shared);
                    return;
                }
            }
        }
        let message: ClientMessage = match decode_line(&line) {
            Ok(m) => m,
            Err(_) => {
                if let Some(c) = shared.clients.lock().unwrap().get(&id) {
                    send(c, error(0, ErrorCode::BadRequest, "invalid request"));
                }
                continue;
            }
        };
        let c = shared.clients.lock().unwrap().get(&id).cloned();
        let Some(c) = c else { return };
        if !greeted {
            match message.request {
                Request::Hello { protocol, .. } if protocol != PROTOCOL_VERSION => {
                    send(
                        &c,
                        error(message.id, ErrorCode::ProtocolMismatch, "protocol mismatch"),
                    );
                    break;
                }
                Request::Hello { .. } => {
                    greeted = true;
                    send(
                        &c,
                        reply(
                            message.id,
                            Response::Welcome {
                                protocol: PROTOCOL_VERSION,
                                daemon: format!("titan-agentd {}", env!("CARGO_PKG_VERSION")),
                            },
                        ),
                    );
                }
                _ => {
                    send(
                        &c,
                        error(message.id, ErrorCode::NotGreeted, "hello required"),
                    );
                    break;
                }
            }
            continue;
        }
        if matches!(message.request, Request::Hello { .. }) {
            send(
                &c,
                error(message.id, ErrorCode::BadRequest, "already greeted"),
            );
            continue;
        }
        let close = handle_request(message, id, &shared);
        if close {
            break;
        }
    }
    cleanup_client(id, &shared);
}
fn cleanup_client(id: u64, s: &Shared) {
    let client = s.clients.lock().unwrap().remove(&id);
    if let Some(c) = &client {
        c.tx.lock().unwrap().take();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !c.writer_done.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if !c.writer_done.load(Ordering::Acquire) {
            let _ = c.stream.shutdown(std::net::Shutdown::Both);
        }
    }
    let (lock, cv) = &s.discovery;
    lock.lock().unwrap().subscribers.remove(&id);
    cv.notify_all();
    let terms: Vec<_> = s
        .registry
        .lock()
        .unwrap()
        .terminals
        .values()
        .cloned()
        .collect();
    for t in terms {
        t.state.lock().unwrap().clients.remove(&id);
    }
}
fn poll_sessions(s: &Shared) {
    let raw = { s.source.lock().unwrap().poll() };
    let _discovery = s.discovery.0.lock().unwrap();
    let mut current = s.sessions.lock().unwrap();
    let next = normalize(raw, Some(&current));
    if *current != next {
        *current = next.clone();
        let clients = s.clients.lock().unwrap();
        for c in clients
            .values()
            .filter(|c| c.subscribed.load(Ordering::Acquire))
        {
            send(
                c,
                ServerMessage::Event {
                    event: Event::Sessions {
                        sessions: next.clone(),
                    },
                },
            );
        }
    }
}

fn discovery_loop(s: Arc<Shared>) {
    loop {
        let (lock, cv) = &s.discovery;
        let mut state = lock.lock().unwrap();
        while state.subscribers.is_empty()
            && !state.requested
            && !s.shutdown.load(Ordering::Acquire)
        {
            state = cv.wait(state).unwrap();
        }
        if s.shutdown.load(Ordering::Acquire) {
            return;
        }
        let immediate = std::mem::take(&mut state.requested);
        if !immediate && !state.subscribers.is_empty() {
            let (next, _) = cv.wait_timeout(state, s.config.poll_interval).unwrap();
            state = next;
            if s.shutdown.load(Ordering::Acquire) {
                return;
            }
            if std::mem::take(&mut state.requested) {
                drop(state);
                poll_sessions(&s);
                continue;
            }
            if state.subscribers.is_empty() {
                continue;
            }
        }
        drop(state);
        poll_sessions(&s);
    }
}

fn handle_request(m: ClientMessage, id: u64, s: &Arc<Shared>) -> bool {
    let c = s.clients.lock().unwrap().get(&id).cloned();
    let Some(c) = c else { return true };
    match m.request {
        Request::Subscribe => {
            let (lock, cv) = &s.discovery;
            let mut discovery = lock.lock().unwrap();
            let sessions = s.sessions.lock().unwrap();
            let rows = sessions.clone();
            send(&c, reply(m.id, Response::Sessions { sessions: rows }));
            c.subscribed.store(true, Ordering::Release);
            if discovery.subscribers.insert(id) {
                discovery.requested = true;
            }
            cv.notify_all();
        }
        Request::Shutdown => {
            send(&c, reply(m.id, Response::Done));
            s.shutdown.store(true, Ordering::Release);
        }
        Request::StopSession { session } => {
            let found = s
                .sessions
                .lock()
                .unwrap()
                .iter()
                .find(|x| x.key == session)
                .cloned();
            match found {
                None => {
                    send(
                        &c,
                        error(m.id, ErrorCode::UnknownSession, "unknown session"),
                    );
                }
                Some(session) => {
                    let result = s.source.lock().unwrap().stop(&session);
                    match result {
                        Ok(()) => {
                            send(&c, reply(m.id, Response::Done));
                            let (lock, cv) = &s.discovery;
                            lock.lock().unwrap().requested = true;
                            cv.notify_all();
                        }
                        Err(StopError::Unsupported(e)) => {
                            send(&c, error(m.id, ErrorCode::Unsupported, &e));
                        }
                        Err(StopError::Failed(e)) => {
                            send(&c, error(m.id, ErrorCode::StopFailed, &e));
                        }
                    }
                }
            }
        }
        Request::OpenTerminal {
            session,
            cols,
            rows,
        } => open_terminal(m.id, id, session, cols, rows, s, &c),
        Request::Input { terminal, data } => {
            let mut result = None;
            let exists = with_terminal(terminal, s, |t| {
                let p = t.process.lock().unwrap().clone();
                result = Some(match p {
                    Some(p) => p.write(&data.0),
                    None => Err(PtyError::Closed),
                });
            });
            if !exists {
                send(
                    &c,
                    error(m.id, ErrorCode::UnknownTerminal, "unknown terminal"),
                );
            } else if let Some(result) = result {
                match result {
                    Ok(()) => {
                        send(&c, reply(m.id, Response::Done));
                    }
                    Err(PtyError::Closed) => {
                        send(
                            &c,
                            error(m.id, ErrorCode::UnknownTerminal, "unknown terminal"),
                        );
                    }
                    Err(PtyError::QueueFull) => {
                        send(
                            &c,
                            error(m.id, ErrorCode::Internal, "terminal input queue is full"),
                        );
                    }
                    Err(_) => {
                        send(
                            &c,
                            error(m.id, ErrorCode::Internal, "terminal input failed"),
                        );
                    }
                }
            }
        }
        Request::Resize {
            terminal,
            cols,
            rows,
        } => {
            if !with_terminal(terminal, s, |t| {
                let mut state = t.state.lock().unwrap();
                state.emulator.resize(cols, rows);
                let p = t.process.lock().unwrap().clone();
                if let Some(p) = p {
                    let _ = p.resize(cols, rows);
                }
                send(&c, reply(m.id, Response::Done));
            }) {
                send(
                    &c,
                    error(m.id, ErrorCode::UnknownTerminal, "unknown terminal"),
                );
            }
        }
        Request::Detach { terminal } => {
            if !with_terminal(terminal, s, |t| {
                t.state.lock().unwrap().clients.remove(&id);
                send(&c, reply(m.id, Response::Done));
            }) {
                send(
                    &c,
                    error(m.id, ErrorCode::UnknownTerminal, "unknown terminal"),
                );
            }
        }
        Request::CloseTerminal { terminal } => {
            let term = s.registry.lock().unwrap().terminals.get(&terminal).cloned();
            if let Some(t) = term {
                let process = t.process.lock().unwrap().clone();
                if let Some(p) = process {
                    p.kill(s.config.kill_grace);
                }
                exit_terminal(&t, None);
                send(&c, reply(m.id, Response::Done));
            } else {
                send(
                    &c,
                    error(m.id, ErrorCode::UnknownTerminal, "unknown terminal"),
                );
            }
        }
        Request::Hello { .. } => {
            send(&c, error(m.id, ErrorCode::BadRequest, "already greeted"));
        }
    }
    false
}
fn with_terminal(id: u64, s: &Shared, f: impl FnOnce(&Arc<Terminal>)) -> bool {
    let t = s.registry.lock().unwrap().terminals.get(&id).cloned();
    if let Some(t) = t {
        let exited = t.state.lock().unwrap().exited;
        if exited {
            return false;
        }
        f(&t);
        return true;
    }
    false
}
fn open_terminal(
    req: u64,
    client: u64,
    key: SessionKey,
    cols: u16,
    rows: u16,
    s: &Arc<Shared>,
    c: &Client,
) {
    let session = s
        .sessions
        .lock()
        .unwrap()
        .iter()
        .find(|x| x.key == key)
        .cloned();
    let Some(session) = session else {
        send(c, error(req, ErrorCode::UnknownSession, "unknown session"));
        return;
    };
    let Some(argv) = session.attach.clone() else {
        send(
            c,
            error(req, ErrorCode::NotAttachable, "session cannot be attached"),
        );
        return;
    };
    let (t, reused) = {
        let mut reg = s.registry.lock().unwrap();
        if reg.closed {
            send(c, error(req, ErrorCode::Internal, "shutting down"));
            return;
        }
        let existing = reg
            .sessions
            .get(&key)
            .and_then(|id| reg.terminals.get(id))
            .cloned();
        if let Some(existing) = existing {
            let exited = existing.state.lock().unwrap().exited;
            if !exited {
                (existing, true)
            } else {
                match spawn_terminal(&mut reg, &session, key.clone(), cols, rows, argv, s) {
                    Ok(t) => (t, false),
                    Err(()) => {
                        send(
                            c,
                            error(req, ErrorCode::SpawnFailed, "failed to start terminal"),
                        );
                        return;
                    }
                }
            }
        } else {
            match spawn_terminal(&mut reg, &session, key.clone(), cols, rows, argv, s) {
                Ok(t) => (t, false),
                Err(()) => {
                    send(
                        c,
                        error(req, ErrorCode::SpawnFailed, "failed to start terminal"),
                    );
                    return;
                }
            }
        }
    };
    let mut state = t.state.lock().unwrap();
    let (actual_cols, actual_rows) = state.emulator.size();
    if !send(
        c,
        reply(
            req,
            Response::TerminalOpened {
                terminal: t.id,
                session: key,
                cols: if reused { actual_cols } else { cols },
                rows: if reused { actual_rows } else { rows },
                reused,
            },
        ),
    ) {
        return;
    }
    let (snap_cols, snap_rows) = state.emulator.size();
    let snapshot = state.emulator.snapshot();
    send(
        c,
        ServerMessage::Event {
            event: Event::TerminalSnapshot {
                terminal: t.id,
                cols: snap_cols,
                rows: snap_rows,
                data: Bytes(snapshot),
            },
        },
    );
    let already_exited = state.exited;
    if !already_exited {
        state.clients.insert(client);
    }
    drop(state);
    if already_exited {
        send(
            c,
            ServerMessage::Event {
                event: Event::TerminalExited {
                    terminal: t.id,
                    code: None,
                },
            },
        );
    }
}

fn spawn_terminal(
    reg: &mut Registry,
    session: &Session,
    key: SessionKey,
    cols: u16,
    rows: u16,
    argv: Vec<String>,
    s: &Shared,
) -> Result<Arc<Terminal>, ()> {
    let id = reg.next;
    reg.next += 1;
    let terminal = Arc::new(Terminal {
        id,
        state: Mutex::new(TermState {
            emulator: Emulator::new(cols, rows, 1000),
            clients: HashSet::new(),
            exited: false,
            pending_replies: Vec::new(),
        }),
        process: Mutex::new(None),
        grace: s.config.kill_grace,
        clients: s.clients.clone(),
        registry: s.registry.clone(),
        cleanup_workers: s.cleanup_workers.clone(),
    });
    let handler = TerminalHandler {
        terminal: terminal.clone(),
    };
    let cwd = std::path::Path::new(&session.cwd)
        .is_dir()
        .then(|| PathBuf::from(&session.cwd));
    match PtyProcess::spawn(
        PtySpawn {
            argv,
            cwd,
            env: Vec::new(),
            cols,
            rows,
        },
        handler,
    ) {
        Ok(p) => {
            let p = Arc::new(p);
            let replies = {
                let mut state = terminal.state.lock().unwrap();
                *terminal.process.lock().unwrap() = Some(p.clone());
                std::mem::take(&mut state.pending_replies)
            };
            for reply in replies {
                let _ = p.write(&reply);
            }
            reg.sessions.insert(key, id);
            reg.terminals.insert(id, terminal.clone());
            Ok(terminal)
        }
        Err(_) => Err(()),
    }
}
struct TerminalHandler {
    terminal: Arc<Terminal>,
}
impl PtyHandler for TerminalHandler {
    fn output(&mut self, bytes: &[u8]) {
        let mut state = self.terminal.state.lock().unwrap();
        if state.exited {
            return;
        }
        let replies = state.emulator.feed(bytes);
        let process = if replies.is_empty() {
            None
        } else {
            let process = self.terminal.process.lock().unwrap().clone();
            if process.is_none() {
                state.pending_replies.push(replies.clone());
            }
            process
        };
        let ids = state.clients.clone();
        for id in ids {
            if let Some(client) = self.terminal.clients.lock().unwrap().get(&id) {
                send(
                    client,
                    ServerMessage::Event {
                        event: Event::TerminalOutput {
                            terminal: self.terminal.id,
                            data: Bytes::from(bytes),
                        },
                    },
                );
            }
        }
        drop(state);
        if let Some(process) = process {
            let _ = process.write(&replies);
        }
    }
    fn exited(&mut self) {
        exit_terminal(&self.terminal, None)
    }
}
fn exit_terminal(t: &Arc<Terminal>, code: Option<i32>) {
    let mut st = t.state.lock().unwrap();
    if st.exited {
        return;
    }
    st.exited = true;
    let ids = st.clients.clone();
    drop(st);
    for id in ids {
        if let Some(c) = t.clients.lock().unwrap().get(&id) {
            send(
                c,
                ServerMessage::Event {
                    event: Event::TerminalExited {
                        terminal: t.id,
                        code,
                    },
                },
            );
        }
    }
    let registry = t.registry.clone();
    let terminal = t.clone();
    let workers = t.cleanup_workers.clone();
    let id = t.id;
    let worker = thread::spawn(move || {
        // Wait for spawn_terminal to finish publishing this terminal.
        drop(registry.lock().unwrap());
        let process = terminal.process.lock().unwrap().clone();
        if let Some(process) = process {
            process.kill(Duration::ZERO);
        }
        let mut registry = registry.lock().unwrap();
        if registry
            .terminals
            .get(&id)
            .is_some_and(|item| item.state.lock().unwrap().exited)
        {
            registry.terminals.remove(&id);
            registry.sessions.retain(|_, terminal| *terminal != id);
        }
    });
    workers.lock().unwrap().push(worker);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(
        state_since_ms: Option<i64>,
        state: titan_agent_core::session::SessionState,
    ) -> Session {
        Session {
            key: SessionKey::new(titan_agent_core::session::AgentCli::Codex, "test"),
            name: "test".into(),
            kind: titan_agent_core::session::SessionKind::Interactive,
            state,
            cwd: String::new(),
            project: titan_agent_core::session::Project {
                name: "test".into(),
                root: String::new(),
            },
            model: None,
            parent: None,
            started_at_ms: Some(0),
            state_since_ms,
            attach: None,
        }
    }

    #[test]
    fn normalize_preserves_or_sets_state_timestamp() {
        let initial = normalize(
            vec![session(
                Some(17),
                titan_agent_core::session::SessionState::Working,
            )],
            None,
        );
        assert_eq!(initial[0].state_since_ms, Some(17));

        let initial_missing = normalize(
            vec![session(
                None,
                titan_agent_core::session::SessionState::Working,
            )],
            None,
        );
        let timestamp = initial_missing[0].state_since_ms.unwrap();
        assert!(timestamp > 0);

        let unchanged = normalize(
            vec![session(
                None,
                titan_agent_core::session::SessionState::Working,
            )],
            Some(&initial_missing),
        );
        assert_eq!(unchanged[0].state_since_ms, Some(timestamp));

        let changed = normalize(
            vec![session(
                None,
                titan_agent_core::session::SessionState::Waiting,
            )],
            Some(&initial_missing),
        );
        assert!(changed[0].state_since_ms.unwrap() >= timestamp);
    }
}
