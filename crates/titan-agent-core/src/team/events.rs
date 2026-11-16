//! Events of the team tool (docs/Decisiones/ADR-0010).
//!
//! `titan-team` appends one JSON object per line to
//! `<state_dir>/events.jsonl`; titan-agentd reads that file to show who talks
//! to whom. Every line carries the format version `v`. A reader skips lines
//! with another version or an unknown `type`, so the format can grow without
//! breaking older readers.

use serde::{Deserialize, Serialize};

/// Version of the event format written by this build.
pub const EVENTS_VERSION: u32 = 1;

/// File name of the event log inside the state folder.
pub const EVENTS_FILE: &str = "events.jsonl";

/// Longest label kept in an event (messages, actions, notes).
pub const LABEL_CHARS: usize = 120;

/// One line of the event log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamEvent {
    /// Format version, [`EVENTS_VERSION`].
    pub v: u32,
    /// Unix time in milliseconds.
    pub at_ms: i64,
    /// Task the event belongs to.
    pub task: String,
    /// Claude Code session id of the director that ran the tool
    /// (`CLAUDE_CODE_SESSION_ID`), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub director: Option<String>,
    #[serde(flatten)]
    pub kind: EventKind,
}

/// What happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// `titan-team new`: the task, its worktree and the team at that moment.
    TaskCreated {
        branch: String,
        base: String,
        /// Folder that holds `.agents/` (where the director runs the tool).
        base_dir: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        team: Option<String>,
        roles: Vec<RoleInfo>,
        #[serde(default)]
        steps: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        review_role: Option<String>,
        #[serde(default)]
        max_review_rounds: u32,
    },
    /// The director sends a turn to a role (`titan-team ask`).
    TurnStarted {
        role: String,
        turn: u32,
        /// True when the role's thread is continued (`codex exec resume`).
        resumed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
        /// Review round this turn opens, for the review role.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        round: Option<u32>,
        /// First words of the director's message, or what the turn is.
        message: String,
    },
    /// What a role is doing during its turn (a command, an edit…), at most
    /// every few seconds.
    Action { role: String, text: String },
    /// The role answered (or failed).
    TurnFinished {
        role: String,
        turn: u32,
        status: TurnStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
        duration_ms: u64,
        #[serde(default)]
        usage: Usage,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
        #[serde(default)]
        changed_files: u32,
        #[serde(default)]
        warnings: u32,
        /// Short answer shown on the arrow back to the director.
        summary: String,
    },
    /// The reviewer's report (after its `turn_finished`).
    Review {
        role: String,
        round: u32,
        max_rounds: u32,
        verdict: String,
        #[serde(default)]
        findings: u32,
    },
    /// A decision or remark of the director (`titan-team note`).
    Note { text: String },
    /// `titan-team close`.
    TaskClosed,
}

/// A role as declared in `team.toml` when the task was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleInfo {
    pub name: String,
    /// Short name shown in the monitor (`label` in `team.toml`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub cli: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// False for the director.
    pub launch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Ok,
    Error,
    Timeout,
}

impl TurnStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Timeout => "timeout",
        }
    }
}

/// Tokens of one turn, as `codex exec --json` reports them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
        self.output_tokens += other.output_tokens;
    }

    /// Tokens shown in the monitor: input plus output.
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

impl TeamEvent {
    pub fn new(task: &str, director: Option<String>, kind: EventKind) -> TeamEvent {
        TeamEvent {
            v: EVENTS_VERSION,
            at_ms: crate::time::now_ms(),
            task: task.to_string(),
            director,
            kind,
        }
    }

    /// One log line, newline included.
    pub fn to_line(&self) -> String {
        let mut line = serde_json::to_string(self).expect("team events always serialize");
        line.push('\n');
        line
    }
}

/// Parses one log line. Returns `None` for blank lines, other versions,
/// unknown types and anything malformed (a reader never fails on a line).
pub fn parse_line(line: &str) -> Option<TeamEvent> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("v").and_then(serde_json::Value::as_u64) != Some(EVENTS_VERSION as u64) {
        return None;
    }
    serde_json::from_value(value).ok()
}

/// Cuts a label to its first line and [`LABEL_CHARS`] characters.
pub fn label(text: &str) -> String {
    let first = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .trim_start_matches('#')
        .trim();
    let mut short: String = first.chars().take(LABEL_CHARS).collect();
    if first.chars().count() > LABEL_CHARS {
        short.push('…');
    }
    short
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_round_trip_as_one_line_with_version_and_type() {
        let event = TeamEvent {
            v: 1,
            at_ms: 1_700_000_000_000,
            task: "demo".into(),
            director: Some("abc".into()),
            kind: EventKind::TurnStarted {
                role: "revisor".into(),
                turn: 1,
                resumed: false,
                thread_id: None,
                round: Some(1),
                message: "Revisa el diff".into(),
            },
        };
        let line = event.to_line();
        assert!(line.ends_with('\n') && !line.trim_end().contains('\n'));
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["v"], 1);
        assert_eq!(value["type"], "turn_started");
        assert_eq!(value["role"], "revisor");
        assert_eq!(parse_line(&line), Some(event));
    }

    #[test]
    fn readers_skip_other_versions_unknown_types_and_garbage() {
        assert!(parse_line("").is_none());
        assert!(parse_line("not json").is_none());
        assert!(parse_line(r#"{"v":2,"at_ms":1,"task":"t","type":"task_closed"}"#).is_none());
        assert!(parse_line(r#"{"v":1,"at_ms":1,"task":"t","type":"mystery"}"#).is_none());
        let closed = parse_line(r#"{"v":1,"at_ms":1,"task":"t","type":"task_closed","new":1}"#);
        assert_eq!(closed.unwrap().kind, EventKind::TaskClosed);
    }

    #[test]
    fn labels_keep_the_first_line_only() {
        assert_eq!(label("\n# Revisa\nresto"), "Revisa");
        let long = "x".repeat(200);
        assert_eq!(label(&long).chars().count(), LABEL_CHARS + 1);
    }
}
