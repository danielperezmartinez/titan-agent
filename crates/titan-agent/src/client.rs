use std::os::unix::{fs::MetadataExt, net::UnixStream, process::CommandExt};
use std::process::Command;
use std::{
    collections::HashMap,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use titan_agent_core::protocol::{
    ClientRequest, DaemonNotification, LineError, LineReader, Message, PROTOCOL_VERSION, PeerInfo,
    RpcError, decode_line, encode_line,
};
#[derive(Debug)]
pub enum ClientError {
    Io(std::io::Error),
    Rpc(RpcError),
    Protocol(String),
    Closed,
    Timeout,
}
struct Inner {
    writer: Mutex<UnixStream>,
    pending: Mutex<HashMap<u64, mpsc::Sender<Result<serde_json::Value, ClientError>>>>,
    next: AtomicU64,
    notices: Mutex<Option<mpsc::Receiver<DaemonNotification>>>,
    notice_tx: Mutex<Option<mpsc::Sender<DaemonNotification>>>,
    closed: std::sync::atomic::AtomicBool,
}
#[derive(Clone)]
pub struct Connection {
    inner: Arc<Inner>,
}
pub fn default_socket_path() -> PathBuf {
    if let Some(s) = std::env::var_os("TITAN_AGENTD_SOCKET").filter(|x| !x.is_empty()) {
        return s.into();
    }
    if let Some(s) = std::env::var_os("XDG_RUNTIME_DIR").filter(|x| !x.is_empty()) {
        return PathBuf::from(s).join("titan-agent/agentd.sock");
    }
    let uid = std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("titan-agent-{uid}/agentd.sock"))
}
pub fn daemon_program() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let sibling = exe.with_file_name("titan-agentd");
    if sibling.is_file() {
        sibling
    } else {
        "titan-agentd".into()
    }
}
impl Connection {
    pub fn connect(socket: &Path) -> Result<Self, ClientError> {
        let stream = UnixStream::connect(socket).map_err(ClientError::Io)?;
        let reader = stream.try_clone().map_err(ClientError::Io)?;
        let (ntx, nrx) = mpsc::channel();
        let inner = Arc::new(Inner {
            writer: Mutex::new(stream),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            notices: Mutex::new(Some(nrx)),
            notice_tx: Mutex::new(Some(ntx)),
            closed: std::sync::atomic::AtomicBool::new(false),
        });
        let c = Self {
            inner: inner.clone(),
        };
        std::thread::spawn(move || reader_loop(reader, inner));
        let result = c.request(ClientRequest::Hello {
            protocol: PROTOCOL_VERSION,
            client: PeerInfo {
                name: "titan-agent".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
        })?;
        let _: serde_json::Value = result;
        Ok(c)
    }
    pub fn request(&self, request: ClientRequest) -> Result<serde_json::Value, ClientError> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(ClientError::Closed);
        }
        let id = self.inner.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.inner.pending.lock().unwrap().insert(id, tx);
        let line = encode_line(&Message::Request { id, request });
        if let Err(e) = self.inner.writer.lock().unwrap().write_all(line.as_bytes()) {
            self.inner.pending.lock().unwrap().remove(&id);
            return Err(ClientError::Io(e));
        }
        rx.recv().unwrap_or(Err(ClientError::Closed))
    }
    pub fn take_notifications(&self) -> Option<mpsc::Receiver<DaemonNotification>> {
        self.inner.notices.lock().unwrap().take()
    }
}
fn reader_loop(stream: UnixStream, inner: Arc<Inner>) {
    let mut r = LineReader::new(BufReader::new(stream));
    loop {
        let msg = match r.next_line() {
            Ok(Some(s)) => decode_line(&s).map_err(|e| ClientError::Protocol(format!("{e:?}"))),
            Ok(None) => break,
            Err(LineError::Io(_)) => break,
            Err(e) => Err(ClientError::Protocol(format!("{e:?}"))),
        };
        match msg {
            Ok(Message::Response { id, result }) => {
                if let Some(tx) = inner.pending.lock().unwrap().remove(&id) {
                    let _ = tx.send(result.map_err(ClientError::Rpc));
                }
            }
            Ok(Message::Notification(n)) => {
                if let Some(tx) = inner.notice_tx.lock().unwrap().as_ref() {
                    let _ = tx.send(n);
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    inner.closed.store(true, Ordering::Release);
    for (_, tx) in inner.pending.lock().unwrap().drain() {
        let _ = tx.send(Err(ClientError::Closed));
    }
    inner.notice_tx.lock().unwrap().take();
}
pub fn connect_or_spawn(
    socket: &Path,
    daemon_program: &Path,
    timeout: Duration,
) -> Result<Connection, ClientError> {
    if let Ok(c) = Connection::connect(socket) {
        return Ok(c);
    }
    let mut command = Command::new(daemon_program);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    let _ = command.spawn();
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(c) = Connection::connect(socket) {
            return Ok(c);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(ClientError::Timeout)
}
