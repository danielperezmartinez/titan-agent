use crate::pty::{PtyHandler, PtyProcess, PtySpawn};
use crate::snapshot::Emulator;
use crate::source::{SessionSource, StopError};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use titan_agent_core::paths::lock_path;
use titan_agent_core::protocol::*;
use titan_agent_core::session::{Session, SessionKey};

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
    tx: SyncSender<Vec<u8>>,
    stream: UnixStream,
    subscribed: AtomicBool,
}
type Clients = Arc<Mutex<HashMap<u64, Arc<Client>>>>;
struct TermState {
    emulator: Emulator,
    clients: HashSet<u64>,
    exited: bool,
}
struct Terminal {
    id: u64,
    state: Mutex<TermState>,
    process: Mutex<Option<Arc<PtyProcess>>>,
    grace: Duration,
    clients: Clients,
}
struct Registry {
    terminals: HashMap<TerminalId, Arc<Terminal>>,
    sessions: HashMap<SessionKey, TerminalId>,
    next: u64,
}
struct Shared {
    clients: Clients,
    registry: Mutex<Registry>,
    sessions: Mutex<Vec<Session>>,
    source: Mutex<Box<dyn SessionSource>>,
    shutdown: Arc<AtomicBool>,
    config: DaemonConfig,
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
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
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
            registry: Mutex::new(Registry {
                terminals: HashMap::new(),
                sessions: HashMap::new(),
                next: 1,
            }),
            sessions: Mutex::new(sessions),
            source: Mutex::new(source),
            shutdown: Arc::new(AtomicBool::new(false)),
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
        while !self.shared.shutdown.load(Ordering::Acquire) {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    let id = NEXT_CLIENT.fetch_add(1, Ordering::Relaxed);
                    let (tx, rx) = mpsc::sync_channel(self.shared.config.client_queue.max(1));
                    let writer = stream.try_clone()?;
                    let client = Arc::new(Client {
                        tx,
                        stream: stream.try_clone()?,
                        subscribed: AtomicBool::new(false),
                    });
                    self.shared.clients.lock().unwrap().insert(id, client);
                    workers.push(thread::spawn(move || writer_loop(writer, rx)));
                    let s = self.shared.clone();
                    workers.push(thread::spawn(move || client_loop(stream, id, s)));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(e) => return Err(e.into()),
            }
            if self
                .shared
                .clients
                .lock()
                .unwrap()
                .values()
                .any(|c| c.subscribed.load(Ordering::Acquire))
            {
                poll_sessions(&self.shared);
                thread::sleep(self.shared.config.poll_interval);
            }
        }
        let clients: Vec<_> = self
            .shared
            .clients
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect();
        for c in &clients {
            let _ = c.stream.shutdown(std::net::Shutdown::Both);
        }
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
                if let Some(p) = t.process.lock().unwrap().clone() {
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
    match client.tx.try_send(encode_line(&msg)) {
        Ok(()) => true,
        Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
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
fn writer_loop(mut stream: UnixStream, rx: mpsc::Receiver<Vec<u8>>) {
    for bytes in rx {
        if stream.write_all(&bytes).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
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
    s.clients.lock().unwrap().remove(&id);
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

fn handle_request(m: ClientMessage, id: u64, s: &Arc<Shared>) -> bool {
    let c = s.clients.lock().unwrap().get(&id).cloned();
    let Some(c) = c else { return true };
    match m.request {
        Request::Subscribe => {
            let sessions = s.sessions.lock().unwrap();
            let rows = sessions.clone();
            send(&c, reply(m.id, Response::Sessions { sessions: rows }));
            c.subscribed.store(true, Ordering::Release);
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
                            poll_sessions(s);
                            send(&c, reply(m.id, Response::Done));
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
        Request::Input { terminal, data } => with_terminal(terminal, s, |t| {
            let p = t.process.lock().unwrap().clone();
            match p.and_then(|p| p.write(&data.0).ok().map(|_| p)) {
                Some(_) => {
                    send(&c, reply(m.id, Response::Done));
                }
                None => {
                    send(
                        &c,
                        error(m.id, ErrorCode::Internal, "terminal input queue is full"),
                    );
                }
            }
        }),
        Request::Resize {
            terminal,
            cols,
            rows,
        } => with_terminal(terminal, s, |t| {
            let p = t.process.lock().unwrap().clone();
            if let Some(p) = p {
                let _ = p.resize(cols, rows);
            }
            let mut state = t.state.lock().unwrap();
            state.emulator.resize(cols, rows);
            send(&c, reply(m.id, Response::Done));
        }),
        Request::Detach { terminal } => with_terminal(terminal, s, |t| {
            t.state.lock().unwrap().clients.remove(&id);
            send(&c, reply(m.id, Response::Done));
        }),
        Request::CloseTerminal { terminal } => {
            let term = s.registry.lock().unwrap().terminals.get(&terminal).cloned();
            if let Some(t) = term {
                if let Some(p) = t.process.lock().unwrap().clone() {
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
fn with_terminal(id: u64, s: &Shared, f: impl FnOnce(&Arc<Terminal>)) {
    let t = s.registry.lock().unwrap().terminals.get(&id).cloned();
    if let Some(t) = t
        && !t.state.lock().unwrap().exited
    {
        f(&t)
    }
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
    let existing = {
        let registry = s.registry.lock().unwrap();
        registry
            .sessions
            .get(&key)
            .and_then(|id| registry.terminals.get(id))
            .cloned()
    };
    let t = if let Some(t) = existing.filter(|t| !t.state.lock().unwrap().exited) {
        t
    } else {
        let id = {
            let mut reg = s.registry.lock().unwrap();
            let id = reg.next;
            reg.next += 1;
            id
        };
        let terminal = Arc::new(Terminal {
            id,
            state: Mutex::new(TermState {
                emulator: Emulator::new(cols, rows, 1000),
                clients: HashSet::new(),
                exited: false,
            }),
            process: Mutex::new(None),
            grace: s.config.kill_grace,
            clients: s.clients.clone(),
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
                *terminal.process.lock().unwrap() = Some(Arc::new(p));
                let mut reg = s.registry.lock().unwrap();
                reg.sessions.insert(key.clone(), id);
                reg.terminals.insert(id, terminal.clone());
            }
            Err(_) => {
                send(
                    c,
                    error(req, ErrorCode::SpawnFailed, "failed to start terminal"),
                );
                return;
            }
        }
        terminal
    };
    let reused = existing_id_for(s, &key) == Some(t.id);
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
    state.clients.insert(client);
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
        if !replies.is_empty()
            && let Some(p) = self.terminal.process.lock().unwrap().clone()
        {
            let _ = p.write(&replies);
        }
    }
    fn exited(&mut self) {
        exit_terminal(&self.terminal, None)
    }
}
fn exit_terminal(t: &Terminal, code: Option<i32>) {
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
}
fn existing_id_for(s: &Shared, key: &SessionKey) -> Option<u64> {
    let r = s.registry.lock().unwrap();
    r.sessions.get(key).copied()
}
