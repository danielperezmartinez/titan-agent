use futures::channel::mpsc as async_mpsc;
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use titan_agent_core::paths::{SOCKET_ENV, socket_path};
use titan_agent_core::protocol::{
    ClientMessage, ErrorBody, Event, FrameError, MAX_FRAME_BYTES, PROTOCOL_VERSION, Request,
    Response, ServerMessage, decode_line, encode_line, read_frame,
};

/// What the daemon sent, in arrival order.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Response {
        id: u64,
        response: Response,
    },
    Error {
        id: u64,
        error: ErrorBody,
    },
    Event(Event),
    /// The connection ended (EOF, I/O or framing error). Sent once, last.
    Disconnected(String),
}

#[derive(Debug)]
pub enum ClientError {
    Connect(io::Error),
    Rejected(ErrorBody),
    Handshake(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(error) => write!(f, "could not connect: {error}"),
            Self::Rejected(error) => write!(f, "daemon rejected Hello: {}", error.message),
            Self::Handshake(message) => write!(f, "Hello handshake failed: {message}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Connect(error) => Some(error),
            _ => None,
        }
    }
}

pub struct DaemonClient {
    writer: Option<mpsc::Sender<Vec<u8>>>,
    next_id: AtomicU64,
    connected: Arc<AtomicBool>,
    socket: UnixStream,
}

impl DaemonClient {
    pub const TIMEOUT: Duration = Duration::from_secs(5);

    pub fn connect(
        socket: &Path,
        client: &str,
    ) -> Result<(Self, String, async_mpsc::UnboundedReceiver<Incoming>), ClientError> {
        let stream = UnixStream::connect(socket).map_err(ClientError::Connect)?;
        let mut handshake_reader =
            BufReader::new(stream.try_clone().map_err(ClientError::Connect)?);
        let hello = ClientMessage {
            id: 1,
            request: Request::Hello {
                protocol: PROTOCOL_VERSION,
                client: client.to_owned(),
            },
        };
        let mut handshake_writer = stream.try_clone().map_err(ClientError::Connect)?;
        handshake_writer
            .write_all(&encode_line(&hello))
            .map_err(ClientError::Connect)?;
        let answer = read_handshake(&stream, &mut handshake_reader, Self::TIMEOUT)?;
        let daemon = match answer {
            ServerMessage::Response {
                id: 1,
                response: Response::Welcome { daemon, .. },
            } => daemon,
            ServerMessage::Error { id: 1, error } => return Err(ClientError::Rejected(error)),
            other => {
                return Err(ClientError::Handshake(format!(
                    "unexpected answer: {other:?}"
                )));
            }
        };

        stream
            .set_read_timeout(None)
            .map_err(ClientError::Connect)?;
        let writer_stream = stream.try_clone().map_err(ClientError::Connect)?;
        // Keep the handshake reader: it may already have buffered frames sent
        // immediately after Welcome.
        let reader = handshake_reader;
        let (writer, writer_rx) = mpsc::channel();
        let (incoming_tx, incoming_rx) = async_mpsc::unbounded();
        let connected = Arc::new(AtomicBool::new(true));
        let disconnected_sent = Arc::new(AtomicBool::new(false));
        spawn_writer(writer_stream, writer_rx, Arc::clone(&connected));
        spawn_reader(
            reader,
            incoming_tx,
            Arc::clone(&connected),
            disconnected_sent,
        );
        Ok((
            Self {
                writer: Some(writer),
                next_id: AtomicU64::new(2),
                connected,
                socket: stream,
            },
            daemon,
            incoming_rx,
        ))
    }

    pub fn connect_or_spawn(
        socket: &Path,
        client: &str,
        spawn: impl FnOnce() -> io::Result<()>,
    ) -> Result<(Self, String, async_mpsc::UnboundedReceiver<Incoming>), ClientError> {
        match Self::connect(socket, client) {
            Ok(value) => Ok(value),
            Err(ClientError::Connect(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                spawn().map_err(ClientError::Connect)?;
                let deadline = Instant::now() + Self::TIMEOUT;
                loop {
                    match Self::connect(socket, client) {
                        Ok(value) => return Ok(value),
                        Err(ClientError::Connect(error))
                            if matches!(
                                error.kind(),
                                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                            ) =>
                        {
                            if Instant::now() >= deadline {
                                return Err(ClientError::Connect(error));
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
            Err(error) => Err(error),
        }
    }

    pub fn send(&self, request: Request) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if self.connected.load(Ordering::Acquire)
            && let Some(writer) = &self.writer
        {
            let message = ClientMessage { id, request };
            let _ = writer.send(encode_line(&message));
        }
        id
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }
}

fn read_handshake(
    stream: &UnixStream,
    reader: &mut BufReader<UnixStream>,
    timeout: Duration,
) -> Result<ServerMessage, ClientError> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ClientError::Handshake(
                "timed out waiting for Hello answer".into(),
            ));
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|error| ClientError::Handshake(error.to_string()))?;
        let mut line = Vec::new();
        let count = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| ClientError::Handshake(error.to_string()))?;
        if Instant::now() >= deadline {
            return Err(ClientError::Handshake(
                "timed out waiting for Hello answer".into(),
            ));
        }
        if count == 0 {
            return Err(ClientError::Handshake("connection closed".into()));
        }
        if line.len() > MAX_FRAME_BYTES {
            return Err(ClientError::Handshake(FrameError::TooLarge.to_string()));
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        return decode_line(&line).map_err(|error| ClientError::Handshake(error.to_string()));
    }
}

impl Drop for DaemonClient {
    fn drop(&mut self) {
        self.writer.take();
        let _ = self.socket.shutdown(Shutdown::Both);
        self.connected.store(false, Ordering::Release);
    }
}

fn disconnect(
    connected: &AtomicBool,
    sent: &AtomicBool,
    tx: &mut async_mpsc::UnboundedSender<Incoming>,
    reason: String,
) {
    connected.store(false, Ordering::Release);
    if !sent.swap(true, Ordering::AcqRel) {
        let _ = tx.unbounded_send(Incoming::Disconnected(reason));
    }
}

fn spawn_writer(stream: UnixStream, receiver: mpsc::Receiver<Vec<u8>>, connected: Arc<AtomicBool>) {
    thread::spawn(move || {
        let mut stream = stream;
        while let Ok(frame) = receiver.recv() {
            if stream.write_all(&frame).is_err() {
                connected.store(false, Ordering::Release);
                let _ = stream.shutdown(Shutdown::Both);
                return;
            }
        }
    });
}

fn spawn_reader(
    mut reader: BufReader<UnixStream>,
    mut tx: async_mpsc::UnboundedSender<Incoming>,
    connected: Arc<AtomicBool>,
    sent: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        loop {
            match read_frame::<_, ServerMessage>(&mut reader) {
                Ok(Some(ServerMessage::Response { id, response })) => {
                    if tx
                        .unbounded_send(Incoming::Response { id, response })
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Some(ServerMessage::Error { id, error })) => {
                    if tx.unbounded_send(Incoming::Error { id, error }).is_err() {
                        break;
                    }
                }
                Ok(Some(ServerMessage::Event { event })) => {
                    if tx.unbounded_send(Incoming::Event(event)).is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    disconnect(&connected, &sent, &mut tx, "connection closed".into());
                    break;
                }
                Err(FrameError::Json(_)) => continue,
                Err(error) => {
                    disconnect(&connected, &sent, &mut tx, error.to_string());
                    break;
                }
            }
        }
        connected.store(false, Ordering::Release);
    });
}

pub fn socket_from_env() -> PathBuf {
    let override_path = std::env::var_os(SOCKET_ENV);
    let runtime = std::env::var_os("XDG_RUNTIME_DIR");
    let uid = std::fs::metadata("/proc/self")
        .map(|metadata| {
            use std::os::unix::fs::MetadataExt;
            metadata.uid()
        })
        .unwrap_or(0);
    socket_path(override_path.as_deref(), runtime.as_deref(), uid)
}

pub fn spawn_daemon(socket: &Path) -> io::Result<()> {
    use std::os::unix::process::CommandExt;
    let executable = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join("titan-agentd")));
    let program = executable
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("titan-agentd"));
    let mut command = Command::new(program);
    command
        .arg("--socket")
        .arg(socket)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    command.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn handshake_deadline_is_absolute_across_blank_lines() {
        let path = std::env::temp_dir().join(format!(
            "titan-client-deadline-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut hello = Vec::new();
            reader.read_until(b'\n', &mut hello).unwrap();
            loop {
                thread::sleep(Duration::from_millis(300));
                if stream.write_all(b"\n").is_err() {
                    break;
                }
            }
        });

        let stream = UnixStream::connect(&path).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let started = Instant::now();
        let result = read_handshake(&stream, &mut reader, Duration::from_millis(900));
        let elapsed = started.elapsed();
        assert!(matches!(result, Err(ClientError::Handshake(_))));
        assert!(elapsed >= Duration::from_millis(800));
        assert!(elapsed < Duration::from_millis(1400));
        let _ = stream.shutdown(Shutdown::Both);
        drop(reader);
        drop(stream);
        server.join().unwrap();
        let _ = std::fs::remove_file(path);
    }
}
