//! A JSON-RPC client for Codex's app-server (ADR-0013): the shared daemon
//! (WebSocket over its unix socket) when it runs, or an own
//! `codex app-server` over stdio (one JSON object per line) for the turn.
//!
//! Requests are written by hand as `{"id":<n>,"method":"…","params":{…}}`
//! so that their first keys are always in that order (FORMAT.md promises
//! it, and the fakes of the tests rely on it).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::process::{kill_tree, own_group};

/// Messages kept while waiting for an answer, at most: a server that
/// floods notifications must not fill the memory (the rest are dropped).
const INBOX_LIMIT: usize = 10_000;

/// How long a setup request (`initialize`, `thread/*`) may take.
pub const SETUP_LIMIT: Duration = Duration::from_secs(120);

/// One request line (FORMAT.md): `{"id":<n>,"method":"…","params":{…}}`.
pub fn request_line(id: u64, method: &str, params: &Value) -> String {
    format!(
        "{{\"id\":{id},\"method\":{},\"params\":{params}}}",
        Value::String(method.into())
    )
}

/// The `initialize` params of titan-team.
pub fn initialize_params() -> Value {
    json!({
        "clientInfo": {"name": "titan-team", "version": env!("CARGO_PKG_VERSION")},
        "capabilities": {"experimentalApi": true},
    })
}

/// Where the shared daemon listens, if `<codex> app-server daemon version`
/// says it runs.
pub fn daemon_socket(program: &str) -> Option<PathBuf> {
    let output = Command::new(program)
        .args(["app-server", "daemon", "version"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    if value.get("status").and_then(Value::as_str) != Some("running") {
        return None;
    }
    value
        .get("socketPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
}

#[cfg(unix)]
type Socket = tungstenite::WebSocket<std::os::unix::net::UnixStream>;

enum Transport {
    Stdio {
        child: Child,
        stdin: Option<ChildStdin>,
        lines: mpsc::Receiver<String>,
        stderr: Option<JoinHandle<Vec<u8>>>,
    },
    #[cfg(unix)]
    Socket(Box<Socket>),
}

/// A connection that already said `initialize`.
pub struct Client {
    transport: Transport,
    next_id: u64,
    /// Messages read while waiting for an answer, not yet handed out.
    inbox: VecDeque<Value>,
}

/// A transport failure (the connection closed or broke), apart from an
/// error answer of Codex.
#[derive(Debug)]
pub struct Closed(pub String);

/// Why [`Client::connect`] failed.
#[derive(Debug)]
pub enum ConnectError {
    /// The program could not be run at all.
    Unavailable(String),
    /// It ran but the greeting failed (with its stderr, if any).
    Failed(String),
}

impl Client {
    /// Connects to the shared daemon if it runs, or starts `<codex>
    /// app-server` in `cwd` with `env`, and says `initialize`.
    pub fn connect(
        program: &str,
        cwd: &Path,
        env: &[(&str, PathBuf)],
    ) -> Result<Client, ConnectError> {
        #[cfg(unix)]
        if let Some(path) = daemon_socket(program)
            && let Ok(client) = Client::socket(&path)
        {
            return client.greet().map_err(ConnectError::Failed);
        }
        Client::stdio(program, cwd, env)
            .map_err(ConnectError::Unavailable)?
            .greet()
            .map_err(ConnectError::Failed)
    }

    #[cfg(unix)]
    fn socket(path: &Path) -> Result<Client, String> {
        let stream =
            std::os::unix::net::UnixStream::connect(path).map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .map_err(|error| error.to_string())?;
        let (socket, _) = tungstenite::client::client("ws://localhost/", stream)
            .map_err(|error| format!("handshake con Codex: {error}"))?;
        Ok(Client {
            transport: Transport::Socket(Box::new(socket)),
            next_id: 1,
            inbox: VecDeque::new(),
        })
    }

    fn stdio(program: &str, cwd: &Path, env: &[(&str, PathBuf)]) -> Result<Client, String> {
        let mut command = Command::new(program);
        command
            .arg("app-server")
            .envs(env.iter().map(|(key, value)| (*key, value)))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        own_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("{program} no está disponible: {error}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            let Some(stdout) = stdout else { return };
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if tx
                            .send(String::from_utf8_lossy(&line).into_owned())
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });
        let mut pipe = child.stderr.take();
        let stderr = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(pipe) = pipe.as_mut() {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        });
        Ok(Client {
            transport: Transport::Stdio {
                child,
                stdin,
                lines,
                stderr: Some(stderr),
            },
            next_id: 1,
            inbox: VecDeque::new(),
        })
    }

    /// Says `initialize` and `initialized`; on failure the connection is
    /// closed (an own server is ended) and its stderr goes in the error.
    fn greet(mut self) -> Result<Client, String> {
        let deadline = Instant::now() + SETUP_LIMIT;
        let error = match self.request("initialize", &initialize_params(), deadline) {
            Ok(Ok(_)) => match self.send(r#"{"method":"initialized"}"#) {
                Ok(()) => return Ok(self),
                Err(Closed(error)) => error,
            },
            Ok(Err(error)) | Err(Closed(error)) => error,
        };
        let stderr = self.close();
        Err(format!("initialize: {error}\n{stderr}")
            .trim_end()
            .to_string())
    }

    fn send(&mut self, line: &str) -> Result<(), Closed> {
        match &mut self.transport {
            Transport::Stdio { stdin, .. } => {
                let Some(stdin) = stdin.as_mut() else {
                    return Err(Closed("la entrada de Codex está cerrada".into()));
                };
                stdin
                    .write_all(format!("{line}\n").as_bytes())
                    .and_then(|()| stdin.flush())
                    .map_err(|error| Closed(format!("Codex cerró la conexión: {error}")))
            }
            #[cfg(unix)]
            Transport::Socket(socket) => socket
                .send(tungstenite::Message::text(line))
                .map_err(|error| Closed(format!("Codex cerró la conexión: {error}"))),
        }
    }

    /// The next message from the server before `deadline`; `Ok(None)` when
    /// the deadline passes first.
    fn read(&mut self, deadline: Instant) -> Result<Option<Value>, Closed> {
        loop {
            // The deadline wins even over a server that never stops writing.
            if Instant::now() >= deadline {
                return Ok(None);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            let text = match &mut self.transport {
                Transport::Stdio { lines, child, .. } => {
                    match lines.recv_timeout(left.min(Duration::from_millis(200))) {
                        Ok(line) => line,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // A server that exited counts as closed, even if
                            // something it left running holds its stdout.
                            if matches!(child.try_wait(), Ok(Some(_))) {
                                match lines.try_recv() {
                                    Ok(line) => line,
                                    Err(_) => {
                                        return Err(Closed("el servidor de Codex salió".into()));
                                    }
                                }
                            } else {
                                continue;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            return Err(Closed("Codex cerró la conexión".into()));
                        }
                    }
                }
                #[cfg(unix)]
                Transport::Socket(socket) => match socket.read() {
                    Ok(tungstenite::Message::Text(text)) => text.to_string(),
                    Ok(tungstenite::Message::Binary(bytes)) => {
                        String::from_utf8_lossy(&bytes).into_owned()
                    }
                    Ok(tungstenite::Message::Close(_)) => {
                        return Err(Closed("Codex cerró la conexión".into()));
                    }
                    Ok(_) => continue,
                    Err(tungstenite::Error::Io(io))
                        if matches!(
                            io.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        if Instant::now() >= deadline {
                            return Ok(None);
                        }
                        continue;
                    }
                    Err(error) => return Err(Closed(error.to_string())),
                },
            };
            if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
                return Ok(Some(value));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
        }
    }

    /// Sends a request and waits for its answer: `Ok(Err(message))` is an
    /// error answer of Codex. Messages that arrive meanwhile are kept for
    /// [`Client::next`].
    pub fn request(
        &mut self,
        method: &str,
        params: &Value,
        deadline: Instant,
    ) -> Result<Result<Value, String>, Closed> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&request_line(id, method, params))?;
        loop {
            let Some(message) = self.read(deadline)? else {
                return Err(Closed(format!("Codex no respondió a {method}")));
            };
            if message.get("method").is_none()
                && message.get("id").and_then(Value::as_u64) == Some(id)
            {
                if let Some(error) = message.get("error") {
                    let text = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("error de Codex");
                    return Ok(Err(text.to_string()));
                }
                return Ok(Ok(message.get("result").cloned().unwrap_or(Value::Null)));
            }
            if self.inbox.len() < INBOX_LIMIT {
                self.inbox.push_back(message);
            }
        }
    }

    /// The next message that is not an answer to a request of ours.
    pub fn next(&mut self, deadline: Instant) -> Result<Option<Value>, Closed> {
        if Instant::now() >= deadline {
            return Ok(None);
        }
        if let Some(message) = self.inbox.pop_front() {
            return Ok(Some(message));
        }
        loop {
            match self.read(deadline)? {
                Some(message) if message.get("method").is_none() && message.get("id").is_some() => {
                    continue;
                }
                other => return Ok(other),
            }
        }
    }

    /// The messages kept by [`Client::request`] and not handed out yet.
    pub fn drain(&mut self) -> Vec<Value> {
        self.inbox.drain(..).collect()
    }

    /// Closes the connection; with an own server, ends it and returns its
    /// stderr.
    pub fn close(self) -> String {
        match self.transport {
            Transport::Stdio {
                mut child,
                stdin,
                stderr,
                ..
            } => {
                drop(stdin);
                let deadline = Instant::now() + Duration::from_secs(3);
                while Instant::now() < deadline {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                // Anything it left running would keep its pipes open.
                kill_tree(&mut child);
                stderr
                    .and_then(|handle| handle.join().ok())
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    .unwrap_or_default()
            }
            #[cfg(unix)]
            Transport::Socket(mut socket) => {
                let _ = socket.close(None);
                let _ = socket.flush();
                String::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_lines_start_with_id_and_method() {
        let line = request_line(7, "turn/start", &json!({"text": "a\nb", "threadId": "t"}));
        assert!(line.starts_with(r#"{"id":7,"method":"turn/start","params":{"#));
        assert!(!line.contains('\n'));
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["params"]["text"], "a\nb");
    }
}
