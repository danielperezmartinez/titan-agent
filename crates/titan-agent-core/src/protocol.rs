//! Local protocol between `titan-agentd` and its clients (ADR-0007).
//!
//! Newline-delimited JSON over a local socket. Every line is one message.
//! Clients send requests (`{"id":1,"method":"attach","params":{...}}`, the
//! `id` is omitted for fire-and-forget requests); the daemon answers with
//! `{"id":1,"result":...}` or `{"id":1,"error":{...}}` and pushes events
//! (`{"event":"sessions","params":{...}}`). Terminal bytes travel in base64.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::launch::LaunchSpec;
use crate::session::{AgentCli, Session};

/// Version of this protocol. A daemon refuses a `hello` with another version.
pub const PROTOCOL_VERSION: u32 = 1;

/// A message from a client to the daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(flatten)]
    pub request: Request,
}

/// Requests understood by the daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    /// First message of every connection. Answered with [`HelloResult`].
    Hello { protocol: u32, client: String },
    /// Start receiving `sessions` events; the current snapshot comes at once.
    Subscribe,
    /// Open (or reuse) the terminal of a session with its official attach
    /// command. Answered with [`AttachResult`]; the terminal history follows
    /// as `output` events.
    Attach {
        session: String,
        cols: u16,
        rows: u16,
    },
    /// Bytes typed by the user, in base64.
    Input { terminal: String, data: String },
    Resize {
        terminal: String,
        cols: u16,
        rows: u16,
    },
    /// Stop receiving a terminal's output. The terminal keeps running.
    Detach { terminal: String },
    /// End the attached terminal (the session itself keeps running).
    CloseTerminal { terminal: String },
    /// Stop the session with its CLI's official command.
    Stop { session: String },
    /// Open the session's working folder in the file manager.
    OpenFolder { session: String },
    /// Launch a new session with the official CLI (ADR-0002). `shown` is the
    /// command line the user saw, which must match the daemon's own
    /// rendering of `spec`; `click` identifies the user's click and is
    /// accepted once. `cols` and `rows` size the terminal of a Codex TUI.
    /// Answered with [`LaunchResult`].
    Launch {
        spec: LaunchSpec,
        shown: String,
        click: String,
        cols: u16,
        rows: u16,
    },
}

/// Answer to [`Request::Hello`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloResult {
    pub protocol: u32,
    pub daemon: String,
}

/// Answer to [`Request::Attach`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachResult {
    pub terminal: String,
    /// False when the terminal already existed (for example after the window
    /// was closed and opened again).
    pub created: bool,
}

/// Answer to [`Request::Launch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchResult {
    pub agent: AgentCli,
    /// Id printed by the CLI (`claude --bg`), when it prints one. The session
    /// shows up in the next `sessions` snapshot with this `cli_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_id: Option<String>,
    /// Terminal opened by the daemon for an interactive TUI (Codex). It is
    /// listed under the provisional session `launch:<terminal>` until the
    /// thread appears, and then under the thread's key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    /// The command that ran, as the user saw it.
    pub command: String,
}

/// Session key under which a launched terminal is listed until its session
/// is discovered.
pub fn provisional_key(terminal: &str) -> String {
    format!("launch:{terminal}")
}

/// An error answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

/// Health of one discovery source, shown in the interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceStatus {
    pub agent: AgentCli,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Events pushed by the daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", content = "params", rename_all = "snake_case")]
pub enum Event {
    /// Full snapshot of the known sessions, sent whenever something changes.
    Sessions {
        sessions: Vec<Session>,
        sources: Vec<SourceStatus>,
        /// Terminals alive in the daemon, as `(session key, terminal id)`.
        terminals: Vec<TerminalRef>,
    },
    /// Output of a terminal, in base64.
    Output { terminal: String, data: String },
    /// The terminal's process ended.
    TerminalClosed {
        terminal: String,
        session: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<u32>,
    },
}

/// A live terminal and the session it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalRef {
    pub session: String,
    pub terminal: String,
}

/// A message from the daemon to a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerMessage {
    Result { id: u64, result: Value },
    Error { id: u64, error: ErrorBody },
    Event(Event),
}

impl ServerMessage {
    pub fn result<T: Serialize>(id: u64, result: &T) -> ServerMessage {
        ServerMessage::Result {
            id,
            result: serde_json::to_value(result).unwrap_or(Value::Null),
        }
    }

    pub fn error(id: u64, code: &str, message: impl Into<String>) -> ServerMessage {
        ServerMessage::Error {
            id,
            error: ErrorBody {
                code: code.to_string(),
                message: message.into(),
            },
        }
    }
}

/// Serializes a message as one protocol line, newline included.
pub fn encode_line<T: Serialize>(message: &T) -> String {
    let mut line = serde_json::to_string(message).expect("protocol messages always serialize");
    line.push('\n');
    line
}

/// Parses one protocol line (with or without its newline).
pub fn decode_line<T: DeserializeOwned>(line: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(line.trim_end_matches(['\n', '\r']))
}

/// Encodes terminal bytes for `input` and `output`.
pub fn encode_bytes(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Decodes terminal bytes from `input` and `output`.
pub fn decode_bytes(data: &str) -> Option<Vec<u8>> {
    STANDARD.decode(data).ok()
}

/// Resolves the daemon socket path. `explicit` is `TITAN_AGENTD_SOCKET`;
/// otherwise the per-user runtime folder is used, with a per-user folder in
/// the temporary directory as a fallback.
pub fn socket_path(
    explicit: Option<&str>,
    runtime_dir: Option<&str>,
    temp_dir: &str,
    user: &str,
) -> String {
    if let Some(path) = explicit.filter(|path| !path.is_empty()) {
        return path.to_string();
    }
    match runtime_dir.filter(|dir| !dir.is_empty()) {
        Some(dir) => format!("{}/titan-agent/agentd.sock", dir.trim_end_matches('/')),
        None => format!(
            "{}/titan-agent-{}/agentd.sock",
            temp_dir.trim_end_matches('/'),
            if user.is_empty() { "user" } else { user }
        ),
    }
}
