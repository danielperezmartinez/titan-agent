//! Connection to titan-agentd. The interface never runs the agent CLIs nor
//! reads their files: everything goes through the daemon (ADR-0004). If the
//! daemon is not running, the interface starts it, detached, so closing the
//! window keeps it (and its terminals) alive.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use titan_agent_core::protocol::{
    ClientMessage, PROTOCOL_VERSION, Request, ServerMessage, decode_line, encode_line,
};

/// What the connection thread reports to the interface.
pub enum Incoming {
    Connected,
    Disconnected(String),
    Message(ServerMessage),
}

#[cfg(unix)]
type Stream = std::os::unix::net::UnixStream;

#[derive(Clone)]
pub struct DaemonClient {
    #[cfg(unix)]
    writer: Arc<Mutex<Option<Stream>>>,
    next_id: Arc<AtomicU64>,
}

impl DaemonClient {
    /// Starts the connection thread. It reconnects (and starts the daemon)
    /// whenever the connection drops.
    pub fn start() -> (DaemonClient, UnboundedReceiver<Incoming>) {
        let (tx, rx) = unbounded();
        let client = DaemonClient {
            #[cfg(unix)]
            writer: Arc::new(Mutex::new(None)),
            next_id: Arc::new(AtomicU64::new(100)),
        };
        let worker = client.clone();
        std::thread::spawn(move || worker.run(tx));
        (client, rx)
    }

    /// Sends a request that expects an answer and returns its id.
    pub fn request(&self, request: Request) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.write(&ClientMessage {
            id: Some(id),
            request,
        });
        id
    }

    /// Sends a request without waiting for an answer.
    pub fn notify(&self, request: Request) {
        self.write(&ClientMessage { id: None, request });
    }

    #[cfg(unix)]
    fn write(&self, message: &ClientMessage) {
        use std::io::Write;
        let line = encode_line(message);
        let mut writer = self.writer.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(stream) = writer.as_mut()
            && stream.write_all(line.as_bytes()).is_err()
        {
            *writer = None;
        }
    }

    #[cfg(not(unix))]
    fn write(&self, _message: &ClientMessage) {}

    #[cfg(unix)]
    fn run(self, tx: UnboundedSender<Incoming>) {
        use std::io::{BufRead, BufReader};
        let socket = socket_path();
        let mut started = false;
        loop {
            let stream = match Stream::connect(&socket) {
                Ok(stream) => stream,
                Err(error) => {
                    if !started {
                        started = true;
                        start_daemon();
                        std::thread::sleep(Duration::from_millis(400));
                        continue;
                    }
                    let _ = tx.unbounded_send(Incoming::Disconnected(format!(
                        "sin conexión con titan-agentd ({error})"
                    )));
                    std::thread::sleep(Duration::from_secs(2));
                    started = false;
                    continue;
                }
            };
            let Ok(reader) = stream.try_clone() else {
                continue;
            };
            *self.writer.lock().unwrap_or_else(|p| p.into_inner()) = Some(stream);
            self.write(&ClientMessage {
                id: Some(1),
                request: Request::Hello {
                    protocol: PROTOCOL_VERSION,
                    client: format!("titan-agent {}", env!("CARGO_PKG_VERSION")),
                },
            });
            self.write(&ClientMessage {
                id: Some(2),
                request: Request::Subscribe,
            });
            let _ = tx.unbounded_send(Incoming::Connected);
            for line in BufReader::new(reader).lines() {
                let Ok(line) = line else { break };
                if let Ok(message) = decode_line::<ServerMessage>(&line)
                    && tx.unbounded_send(Incoming::Message(message)).is_err()
                {
                    return;
                }
            }
            *self.writer.lock().unwrap_or_else(|p| p.into_inner()) = None;
            if tx
                .unbounded_send(Incoming::Disconnected(
                    "se perdió la conexión con titan-agentd".into(),
                ))
                .is_err()
            {
                return;
            }
            started = false;
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    #[cfg(not(unix))]
    fn run(self, tx: UnboundedSender<Incoming>) {
        let _ = tx.unbounded_send(Incoming::Disconnected(
            "titan-agentd aún no funciona en Windows (named pipe pendiente)".into(),
        ));
    }
}

fn socket_path() -> std::path::PathBuf {
    let explicit = std::env::var("TITAN_AGENTD_SOCKET").ok();
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok();
    let user = std::env::var("USER").unwrap_or_default();
    std::path::PathBuf::from(titan_agent_core::protocol::socket_path(
        explicit.as_deref(),
        runtime.as_deref(),
        &std::env::temp_dir().to_string_lossy(),
        &user,
    ))
}

/// Starts `titan-agentd` (next to this executable, or from `PATH`) in its own
/// process group so it outlives the window.
#[cfg(unix)]
fn start_daemon() {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("titan-agentd")))
        .filter(|path| path.is_file());
    let program = sibling.unwrap_or_else(|| "titan-agentd".into());
    let log = socket_path().with_extension("log");
    if let Some(dir) = log.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let stderr = std::fs::File::create(&log)
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null());
    let spawned = Command::new(program)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .process_group(0)
        .spawn();
    if let Ok(mut child) = spawned {
        // Reap it if it exits early (for example, another daemon won).
        std::thread::spawn(move || child.wait());
    }
}
