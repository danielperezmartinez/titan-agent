//! Read-only client of the shared `codex app-server` daemon.
//!
//! The daemon listens on a Unix socket and speaks JSON-RPC over WebSocket.
//! titan-agentd only lists threads and, when the user presses `[x] detener`,
//! interrupts the running turn: it never starts turns nor sends text.

use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use titan_agent_core::discovery::codex_session;
use titan_agent_core::session::Session;

/// Threads that are not loaded are still shown for this long after their last
/// update, so a team run that just ended stays visible under its director.
pub const RECENT_WINDOW_S: i64 = 20 * 60;

/// Source kinds listed (every kind except the internal review and compact
/// sub-agents).
const SOURCE_KINDS: [&str; 6] = [
    "cli",
    "vscode",
    "exec",
    "appServer",
    "subAgent",
    "subAgentThreadSpawn",
];

/// Asks the official CLI where the shared daemon listens
/// (`codex app-server daemon version`).
pub fn socket_path(codex: &str) -> Result<PathBuf, String> {
    let output = Command::new(codex)
        .args(["app-server", "daemon", "version"])
        .output()
        .map_err(|error| format!("no se pudo ejecutar {codex}: {error}"))?;
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "codex app-server daemon version no devolvió JSON".to_string())?;
    if value.get("status").and_then(Value::as_str) != Some("running") {
        return Err("el daemon de Codex no está en marcha".into());
    }
    value
        .get("socketPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "el daemon de Codex no indica su socket".into())
}

#[cfg(unix)]
type Stream = std::os::unix::net::UnixStream;

#[cfg(unix)]
pub struct CodexClient {
    socket: tungstenite::WebSocket<Stream>,
    next_id: u64,
}

#[cfg(unix)]
impl CodexClient {
    pub fn connect(path: &std::path::Path) -> Result<CodexClient, String> {
        let stream = Stream::connect(path).map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|error| error.to_string())?;
        let (socket, _) = tungstenite::client::client("ws://localhost/", stream)
            .map_err(|error| format!("handshake con Codex: {error}"))?;
        let mut client = CodexClient { socket, next_id: 1 };
        client.request(
            "initialize",
            json!({"clientInfo": {"name": "titan-agentd", "version": env!("CARGO_PKG_VERSION")}}),
        )?;
        client.notify("initialized")?;
        Ok(client)
    }

    fn notify(&mut self, method: &str) -> Result<(), String> {
        let text = json!({"method": method}).to_string();
        self.socket
            .send(tungstenite::Message::text(text))
            .map_err(|error| error.to_string())
    }

    /// Sends a request and waits for its answer, skipping notifications.
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let text = json!({"id": id, "method": method, "params": params}).to_string();
        self.socket
            .send(tungstenite::Message::text(text))
            .map_err(|error| error.to_string())?;
        loop {
            let message = self.socket.read().map_err(|error| match error {
                tungstenite::Error::Io(io) if io.kind() == io::ErrorKind::WouldBlock => {
                    "Codex no responde".to_string()
                }
                other => other.to_string(),
            })?;
            let text = match message {
                tungstenite::Message::Text(text) => text.to_string(),
                tungstenite::Message::Binary(bytes) => String::from_utf8_lossy(&bytes).into(),
                tungstenite::Message::Close(_) => return Err("Codex cerró la conexión".into()),
                _ => continue,
            };
            let Ok(value) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("error de Codex")
                    .to_string());
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// Lists the threads to show: every loaded thread and the ones updated in
    /// the last [`RECENT_WINDOW_S`] seconds.
    pub fn sessions(&mut self) -> Result<Vec<Session>, String> {
        let loaded: HashSet<String> = self
            .request("thread/loaded/list", json!({}))?
            .get("data")
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let recent = self.request(
            "thread/list",
            json!({"limit": 60, "sortKey": "updated_at", "sourceKinds": SOURCE_KINDS}),
        )?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let mut sessions = Vec::new();
        let mut seen = HashSet::new();
        for thread in recent
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(id) = thread.get("id").and_then(Value::as_str) else {
                continue;
            };
            let is_loaded = loaded.contains(id);
            let updated = thread.get("updatedAt").and_then(Value::as_i64).unwrap_or(0);
            if !is_loaded && now - updated > RECENT_WINDOW_S {
                continue;
            }
            if let Some(session) = codex_session(thread, is_loaded) {
                seen.insert(id.to_string());
                sessions.push(session);
            }
        }
        for id in loaded.iter().filter(|id| !seen.contains(*id)) {
            if let Ok(read) = self.request("thread/read", json!({"threadId": id}))
                && let Some(session) = read.get("thread").and_then(|t| codex_session(t, true))
            {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }

    /// Interrupts the turn in progress of a thread, if there is one.
    pub fn interrupt(&mut self, thread: &str) -> Result<bool, String> {
        let turns = self.request("thread/turns/list", json!({"threadId": thread, "limit": 1}))?;
        let running = turns
            .get("data")
            .and_then(Value::as_array)
            .and_then(|turns| turns.first())
            .filter(|turn| turn.get("status").and_then(Value::as_str) == Some("inProgress"))
            .and_then(|turn| turn.get("id").and_then(Value::as_str))
            .map(str::to_string);
        match running {
            Some(turn) => {
                self.request(
                    "turn/interrupt",
                    json!({"threadId": thread, "turnId": turn}),
                )?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[cfg(not(unix))]
pub struct CodexClient;

#[cfg(not(unix))]
impl CodexClient {
    pub fn connect(_path: &std::path::Path) -> Result<CodexClient, String> {
        Err("el daemon de Codex aún no se lee en Windows".into())
    }

    pub fn sessions(&mut self) -> Result<Vec<Session>, String> {
        Ok(Vec::new())
    }

    pub fn interrupt(&mut self, _thread: &str) -> Result<bool, String> {
        Ok(false)
    }
}
