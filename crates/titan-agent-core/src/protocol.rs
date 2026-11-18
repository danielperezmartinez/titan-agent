//! Local protocol between `titan-agentd` and its clients (ADR-0007).
//!
//! Newline-delimited JSON over a local socket. Every line is one message.
//! Clients send requests (`{"id":1,"method":"attach","params":{...}}`, the
//! `id` is omitted for fire-and-forget requests); the daemon answers with
//! `{"id":1,"result":...}` or `{"id":1,"error":{...}}` and pushes events
//! (`{"event":"sessions","params":{...}}`). Terminal bytes travel in base64.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::launch::LaunchSpec;
use crate::quota::Quota;
use crate::session::{AgentCli, Session};
use crate::transcript::TranscriptItem;

/// Version of this protocol. A daemon refuses a `hello` with another version.
pub const PROTOCOL_VERSION: u32 = 1;

/// A message from a client to the daemon.
///
/// Decoding refuses unknown fields in the `params` of a `launch`, so a
/// client cannot ask for anything the user did not see (a repeat, a delay…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Value")]
pub struct ClientMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(flatten)]
    pub request: Request,
}

/// Fields of the `launch` params; anything else is refused.
const LAUNCH_FIELDS: [&str; 3] = ["token", "spec", "command"];

impl TryFrom<Value> for ClientMessage {
    type Error = String;

    fn try_from(value: Value) -> Result<ClientMessage, String> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(default)]
            id: Option<u64>,
            #[serde(flatten)]
            request: Request,
        }
        if value.get("method").and_then(Value::as_str) == Some("launch")
            && let Some(params) = value.get("params").and_then(Value::as_object)
            && let Some(unknown) = params.keys().find(|k| !LAUNCH_FIELDS.contains(&k.as_str()))
        {
            return Err(format!("unknown field `{unknown}` in launch"));
        }
        let mut value = value;
        // `update_apply` may come without params: `force` is then false.
        if value.get("method").and_then(Value::as_str) == Some("update_apply")
            && value.get("params").is_none_or(Value::is_null)
            && let Some(object) = value.as_object_mut()
        {
            object.insert("params".into(), serde_json::json!({}));
        }
        let wire: Wire = serde_json::from_value(value).map_err(|error| error.to_string())?;
        Ok(ClientMessage {
            id: wire.id,
            request: wire.request,
        })
    }
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
    /// Launch a new session with the official CLI (ADR-0002). `token` is
    /// unique per click of the user; `command` is the line the user saw and
    /// must equal `spec.command_line()`. Answered with [`LaunchResult`].
    Launch {
        token: String,
        spec: LaunchSpec,
        command: String,
    },
    /// Read-only transcript of a Codex thread (`cx:<thread>`), listed or
    /// not, from the official app-server. Opens no writer on the thread.
    /// Answered with [`TranscriptResult`].
    Transcript { session: String },
    /// State of the updater (ADR-0011 §6). Answered with [`UpdateStatus`].
    UpdateStatus,
    /// Look for a new version now, even with automatic checks off.
    UpdateCheck,
    /// Apply the prepared version now (the «actualizar» button) or restart
    /// into the installed one. `force` confirms closing the terminals of
    /// working sessions.
    UpdateApply {
        #[serde(default)]
        force: bool,
    },
    /// Turn the automatic checks on or off. Answered with [`UpdateStatus`].
    UpdateSettings { auto: bool },
    /// Forget the notice of the last rollback.
    UpdateDismiss,
    /// Import a folder as a project (ADR-0014): its `project_root`, kept in
    /// `projects.json`. Answered with [`ProjectAddResult`]; errors
    /// `not_a_folder` and `config_error`.
    ProjectAdd { path: String },
    /// Forget an imported project; its folder is never touched. Answered
    /// with `{}`, also when it was not imported.
    ProjectRemove { root: String },
}

/// Answer of `project_add`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAddResult {
    pub root: String,
}

/// One imported project as the `sessions` event carries it (ADR-0014).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub root: String,
    /// Last component of the root.
    pub name: String,
    pub added_ms: i64,
    /// The folder is gone (or is not a folder).
    pub missing: bool,
}

/// State of the updater of the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Downloading,
    /// A newer version is prepared and applies when it is safe.
    Ready,
    /// Ready, but waiting for working terminals to finish (no window).
    Waiting,
    Applying,
    Failed,
}

/// Answer to the `update_*` requests and params of the `update` event.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub state: UpdateState,
    /// False in a build that is not installed or with
    /// `TITAN_AGENT_NO_UPDATE`: nothing is checked nor applied.
    pub active: bool,
    /// Version of the daemon that answers.
    pub installed: String,
    /// Version `current` points to, when installed.
    pub current: Option<String>,
    /// The new version (downloading, ready, waiting, applying or failed).
    pub version: Option<String>,
    pub notes_url: Option<String>,
    /// Sessions whose terminals would close (working, with a terminal here).
    pub blocking: Vec<String>,
    pub error: Option<String>,
    /// Automatic checks are on.
    pub auto: bool,
    /// Unix milliseconds of the last finished check.
    pub last_check_ms: Option<i64>,
    /// The launcher went back from a version that did not start.
    pub rolled_back: Option<crate::update::RolledBack>,
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
    /// Provisional session key (`launch:<n>`) used until the CLI reports the
    /// real session; see [`LaunchRef`].
    pub launch: String,
    pub agent: AgentCli,
    /// Terminal of the launched TUI (Codex); `None` for Claude Code, which
    /// runs in the background and is attached like any other session.
    pub terminal: Option<String>,
    /// Id printed by `claude --bg`.
    pub cli_id: Option<String>,
}

/// Answer to [`Request::Transcript`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptResult {
    /// The key asked for.
    pub session: String,
    /// Oldest first, at most [`crate::transcript::TRANSCRIPT_ITEMS`].
    pub items: Vec<TranscriptItem>,
    /// Older items exist that are not sent.
    pub more: bool,
    /// Working folder of the thread, as Codex reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// A launch and, once known, the real session it became.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchRef {
    pub launch: String,
    pub session: Option<String>,
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
        /// Sessions launched by this daemon and the real session each one
        /// became, once the CLI reports it.
        #[serde(default)]
        launches: Vec<LaunchRef>,
        /// Open team tasks of the known projects (docs/Decisiones/ADR-0010).
        #[serde(default)]
        teams: Vec<TeamTask>,
        /// Remaining quota of each CLI that reports it.
        #[serde(default)]
        quotas: Vec<Quota>,
        /// Imported projects, in the order of `projects.json` (ADR-0014).
        /// `None` from a daemon that does not know about them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        projects: Option<Vec<ProjectInfo>>,
        /// Why `projects.json` could not be read, if it could not.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        projects_error: Option<String>,
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
    /// The updater changed state (ADR-0011 §6).
    Update(UpdateStatus),
}

/// The events of one open team task, as written by `titan-team`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamTask {
    /// Project the team folder belongs to (worktrees folded).
    pub project_root: String,
    /// Folder that holds `.agents/team.toml`.
    pub folder: String,
    pub task: String,
    /// Raw event objects of `events.jsonl`, in file order (the last
    /// [`TEAM_EVENTS_SENT`] at most).
    pub events: Vec<Value>,
    /// The task's `task_created` event, kept even when it is no longer
    /// among the last events (additive field, ADR-0010).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<Value>,
    /// Finished turns and tokens of each role over the whole file, which
    /// the last events alone may undercount (additive field, ADR-0010).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub totals: BTreeMap<String, RoleTotals>,
    /// Last `director_session` over the whole file, which the last events
    /// alone may no longer carry (additive field, ADR-0010).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub director_session: Option<String>,
}

/// Counters of one role over the whole events file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoleTotals {
    pub turns: u32,
    /// Input plus output tokens of its finished turns.
    pub tokens: u64,
}

/// Events of a task sent in each snapshot, at most.
pub const TEAM_EVENTS_SENT: usize = 300;

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
    Result {
        id: u64,
        result: Value,
    },
    /// `id` is `null` when the request line could not be parsed at all
    /// (the JSON-RPC convention).
    Error {
        id: Option<u64>,
        error: ErrorBody,
    },
    Event(Event),
}

impl ServerMessage {
    pub fn result<T: Serialize>(id: u64, result: &T) -> ServerMessage {
        ServerMessage::Result {
            id,
            result: serde_json::to_value(result).unwrap_or(Value::Null),
        }
    }

    /// Error for a line that could not be parsed, so its id is unknown.
    pub fn bad_line(message: impl Into<String>) -> ServerMessage {
        ServerMessage::Error {
            id: None,
            error: ErrorBody {
                code: "bad_request".to_string(),
                message: message.into(),
            },
        }
    }

    pub fn error(id: u64, code: &str, message: impl Into<String>) -> ServerMessage {
        ServerMessage::Error {
            id: Some(id),
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
    socket_path_named(explicit, runtime_dir, temp_dir, user, socket_name(false))
}

/// File name of the default socket. A debug build uses its own, so a
/// development daemon never takes the place of the installed one
/// (ADR-0011 §4). Callers pass `cfg!(debug_assertions)` of their binary.
pub fn socket_name(debug: bool) -> &'static str {
    if debug {
        "agentd-dev.sock"
    } else {
        "agentd.sock"
    }
}

/// [`socket_path`] with the socket file `name`.
pub fn socket_path_named(
    explicit: Option<&str>,
    runtime_dir: Option<&str>,
    temp_dir: &str,
    user: &str,
    name: &str,
) -> String {
    if let Some(path) = explicit.filter(|path| !path.is_empty()) {
        return path.to_string();
    }
    match runtime_dir.filter(|dir| !dir.is_empty()) {
        Some(dir) => format!("{}/titan-agent/{name}", dir.trim_end_matches('/')),
        None => format!(
            "{}/titan-agent-{}/{name}",
            temp_dir.trim_end_matches('/'),
            if user.is_empty() { "user" } else { user }
        ),
    }
}

/// The daemon writes its pid in this file, in the folder of its socket.
pub const PID_FILE: &str = "agentd.pid";
