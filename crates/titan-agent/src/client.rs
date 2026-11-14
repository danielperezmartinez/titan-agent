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
    control: UnixStream,
    state: Mutex<PendingState>,
    next: AtomicU64,
    notices: Mutex<Option<mpsc::Receiver<DaemonNotification>>>,
    notice_tx: Mutex<Option<mpsc::Sender<DaemonNotification>>>,
    tagged_notices: Mutex<Option<mpsc::Receiver<(u64, DaemonNotification)>>>,
    tagged_notice_tx: Mutex<Option<mpsc::Sender<(u64, DaemonNotification)>>>,
    notification_sequence: AtomicU64,
}
struct PendingState {
    closed: bool,
    pending: HashMap<u64, mpsc::Sender<Result<serde_json::Value, ClientError>>>,
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
        Self::connect_with_deadline(socket, None)
    }
    fn connect_with_deadline(
        socket: &Path,
        deadline: Option<Instant>,
    ) -> Result<Self, ClientError> {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(ClientError::Timeout);
        }
        let stream = UnixStream::connect(socket).map_err(ClientError::Io)?;
        let reader = stream.try_clone().map_err(ClientError::Io)?;
        let control = stream.try_clone().map_err(ClientError::Io)?;
        let (ntx, nrx) = mpsc::channel();
        let (ttx, trx) = mpsc::channel();
        let inner = Arc::new(Inner {
            writer: Mutex::new(stream),
            control,
            state: Mutex::new(PendingState {
                closed: false,
                pending: HashMap::new(),
            }),
            next: AtomicU64::new(1),
            notices: Mutex::new(Some(nrx)),
            notice_tx: Mutex::new(Some(ntx)),
            tagged_notices: Mutex::new(Some(trx)),
            tagged_notice_tx: Mutex::new(Some(ttx)),
            notification_sequence: AtomicU64::new(0),
        });
        let c = Self {
            inner: inner.clone(),
        };
        std::thread::spawn(move || reader_loop(reader, inner));
        let result = match c.request_until(
            ClientRequest::Hello {
                protocol: PROTOCOL_VERSION,
                client: PeerInfo {
                    name: "titan-agent".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                },
            },
            deadline,
        ) {
            Ok(result) => result,
            Err(error) => {
                c.close();
                return Err(error);
            }
        };
        let _: serde_json::Value = result;
        Ok(c)
    }
    pub fn request(&self, request: ClientRequest) -> Result<serde_json::Value, ClientError> {
        self.request_until(request, None)
    }
    fn request_until(
        &self,
        request: ClientRequest,
        deadline: Option<Instant>,
    ) -> Result<serde_json::Value, ClientError> {
        let id = self.inner.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        {
            let mut state = self.inner.state.lock().unwrap();
            if state.closed {
                return Err(ClientError::Closed);
            }
            state.pending.insert(id, tx);
        }
        let line = encode_line(&Message::Request { id, request });
        let write_result = self.inner.writer.lock().unwrap().write_all(line.as_bytes());
        if write_result.is_err() {
            self.close();
            return Err(ClientError::Closed);
        }
        match deadline {
            Some(deadline) => match deadline.checked_duration_since(Instant::now()) {
                Some(remaining) => match rx.recv_timeout(remaining) {
                    Ok(result) => result,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        self.close();
                        Err(ClientError::Timeout)
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => Err(ClientError::Closed),
                },
                None => {
                    self.close();
                    Err(ClientError::Timeout)
                }
            },
            None => rx.recv().unwrap_or(Err(ClientError::Closed)),
        }
    }
    pub fn take_notifications(&self) -> Option<mpsc::Receiver<DaemonNotification>> {
        let receiver = self.inner.notices.lock().unwrap().take();
        if receiver.is_some() {
            self.inner.tagged_notices.lock().unwrap().take();
        }
        receiver
    }
    pub(crate) fn take_notification_events(
        &self,
    ) -> Option<mpsc::Receiver<(u64, DaemonNotification)>> {
        let receiver = self.inner.tagged_notices.lock().unwrap().take();
        if receiver.is_some() {
            self.inner.notices.lock().unwrap().take();
        }
        receiver
    }
    pub(crate) fn notification_sequence(&self) -> u64 {
        self.inner.notification_sequence.load(Ordering::Acquire)
    }
    fn close(&self) {
        close_inner(&self.inner);
    }
}
fn close_inner(inner: &Arc<Inner>) {
    let mut state = inner.state.lock().unwrap();
    if state.closed {
        return;
    }
    state.closed = true;
    for (_, tx) in state.pending.drain() {
        let _ = tx.send(Err(ClientError::Closed));
    }
    drop(state);
    inner.notice_tx.lock().unwrap().take();
    inner.tagged_notice_tx.lock().unwrap().take();
    let _ = inner.control.shutdown(std::net::Shutdown::Both);
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
                if let Some(tx) = inner.state.lock().unwrap().pending.remove(&id) {
                    let _ = tx.send(result.map_err(ClientError::Rpc));
                }
            }
            Ok(Message::Notification(n)) => {
                let sequence = inner.notification_sequence.fetch_add(1, Ordering::AcqRel) + 1;
                if let Some(tx) = inner.notice_tx.lock().unwrap().as_ref() {
                    let _ = tx.send(n.clone());
                }
                if let Some(tx) = inner.tagged_notice_tx.lock().unwrap().as_ref() {
                    let _ = tx.send((sequence, n));
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    close_inner(&inner);
}
pub fn connect_or_spawn(
    socket: &Path,
    daemon_program: &Path,
    timeout: Duration,
) -> Result<Connection, ClientError> {
    let deadline = Instant::now() + timeout;
    if let Ok(c) = Connection::connect_with_deadline(socket, Some(deadline)) {
        return Ok(c);
    }
    if Instant::now() >= deadline {
        return Err(ClientError::Timeout);
    }
    let mut command = Command::new(daemon_program);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    let _ = command.spawn();
    while Instant::now() < deadline {
        if let Ok(c) = Connection::connect_with_deadline(socket, Some(deadline)) {
            return Ok(c);
        }
        std::thread::sleep(
            Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
    Err(ClientError::Timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::{UnixListener, UnixStream};

    fn connection(stream: UnixStream) -> Connection {
        let (notice_tx, notice_rx) = mpsc::channel();
        let control = stream.try_clone().unwrap();
        Connection {
            inner: Arc::new(Inner {
                writer: Mutex::new(stream),
                control,
                state: Mutex::new(PendingState {
                    closed: false,
                    pending: HashMap::new(),
                }),
                next: AtomicU64::new(1),
                notices: Mutex::new(Some(notice_rx)),
                notice_tx: Mutex::new(Some(notice_tx)),
                tagged_notices: Mutex::new(None),
                tagged_notice_tx: Mutex::new(None),
                notification_sequence: AtomicU64::new(0),
            }),
        }
    }

    #[test]
    fn write_failure_closes_connection_and_releases_every_pending_request() {
        use std::io::BufRead;
        let (stream, peer) = UnixStream::pair().unwrap();
        let connection = Arc::new(connection(stream));
        let (result_tx, result_rx) = mpsc::channel();
        let mut workers = Vec::new();
        for _ in 0..2 {
            let connection = connection.clone();
            let result_tx = result_tx.clone();
            workers.push(std::thread::spawn(move || {
                let _ = result_tx.send(connection.request(ClientRequest::SessionsSubscribe));
            }));
        }
        let mut reader = BufReader::new(peer.try_clone().unwrap());
        for _ in 0..2 {
            let mut line = String::new();
            assert_ne!(reader.read_line(&mut line).unwrap(), 0);
        }
        peer.shutdown(std::net::Shutdown::Both).unwrap();
        assert!(matches!(
            connection.request(ClientRequest::SessionsSubscribe),
            Err(ClientError::Closed)
        ));
        for _ in 0..2 {
            assert!(matches!(
                result_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
                Err(ClientError::Closed)
            ));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        let notices = connection.take_notifications().unwrap();
        assert!(notices.recv().is_err());
        assert!(connection.take_notifications().is_none());
    }

    #[test]
    fn connect_or_spawn_deadline_includes_a_daemon_that_never_answers_hello() {
        let path =
            std::env::temp_dir().join(format!("titan-agent-timeout-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            if let Ok((_stream, _)) = listener.accept() {
                std::thread::sleep(Duration::from_millis(400));
            }
        });
        let began = Instant::now();
        let result = connect_or_spawn(
            &path,
            Path::new("/missing/titan-agentd"),
            Duration::from_millis(120),
        );
        assert!(matches!(result, Err(ClientError::Timeout)));
        assert!(began.elapsed() < Duration::from_millis(350));
        let _ = std::fs::remove_file(&path);
        let _ = server.join();
    }

    #[test]
    fn notifications_do_not_extend_hello_deadline() {
        let path = std::env::temp_dir().join(format!(
            "titan-agent-notify-timeout-{}.sock",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut hello = String::new();
            std::io::BufRead::read_line(&mut reader, &mut hello).unwrap();
            let notification = encode_line(&Message::Notification(
                DaemonNotification::SessionsChanged { sessions: vec![] },
            ));
            loop {
                std::thread::sleep(Duration::from_millis(50));
                if stream.write_all(notification.as_bytes()).is_err() {
                    break;
                }
            }
        });
        let began = Instant::now();
        let result =
            Connection::connect_with_deadline(&path, Some(began + Duration::from_millis(180)));
        assert!(matches!(result, Err(ClientError::Timeout)));
        assert!(began.elapsed() < Duration::from_millis(350));
        let _ = server.join();
        let _ = std::fs::remove_file(&path);
    }
}
