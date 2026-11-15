use std::fmt;

use serde::{Deserialize, Serialize};

/// The official CLI used to run an agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCli {
    ClaudeCode,
    Codex,
}

impl AgentCli {
    /// Every variant, in display order.
    pub const ALL: [AgentCli; 2] = [AgentCli::ClaudeCode, AgentCli::Codex];

    /// Returns the executable name of the official CLI.
    pub fn command(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
        }
    }

    /// Returns the two-letter tag shown in the agent pill.
    pub fn tag(self) -> &'static str {
        match self {
            Self::ClaudeCode => "cl",
            Self::Codex => "cx",
        }
    }

    /// Finds a CLI by its exact executable name.
    pub fn from_command(command: &str) -> Option<AgentCli> {
        match command {
            "claude" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

impl fmt::Display for AgentCli {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
        })
    }
}

/// The current state of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// The agent is working.
    Working,
    /// The agent is waiting for the user (a question or a permission).
    NeedsYou,
    /// Idle, waiting for a new message.
    Waiting,
    /// The session ended; `failed` is true when it ended with an error.
    Finished { failed: bool },
}

impl SessionState {
    /// Returns the ASCII marker for this state.
    pub fn marker(self) -> &'static str {
        match self {
            Self::Working => "[>]",
            Self::NeedsYou => "[?]",
            Self::Waiting => "[+]",
            Self::Finished { .. } => "[x]",
        }
    }

    /// Returns the Spanish label shown next to the marker.
    pub fn label(self) -> &'static str {
        match self {
            Self::Working => "trabajando",
            Self::NeedsYou => "te necesita",
            Self::Waiting => "en espera",
            Self::Finished { failed: false } => "terminada",
            Self::Finished { failed: true } => "terminada con error",
        }
    }

    /// Returns whether this session is still live.
    pub fn is_live(self) -> bool {
        !matches!(self, Self::Finished { .. })
    }
}

/// Global counts of live session states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StateCounts {
    pub working: usize,
    pub needs_you: usize,
    pub waiting: usize,
}

impl StateCounts {
    /// Counts states, excluding finished sessions.
    pub fn tally<I: IntoIterator<Item = SessionState>>(states: I) -> StateCounts {
        let mut counts = Self::default();
        for state in states {
            match state {
                SessionState::Working => counts.working += 1,
                SessionState::NeedsYou => counts.needs_you += 1,
                SessionState::Waiting => counts.waiting += 1,
                SessionState::Finished { .. } => {}
            }
        }
        counts
    }

    /// Returns the total number of live sessions.
    pub fn total(&self) -> usize {
        self.working + self.needs_you + self.waiting
    }
}

/// How a session was started, as reported by its CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    /// An interactive CLI running in some other terminal.
    Interactive,
    /// A Claude Code background session (`claude --bg`), attachable.
    Background,
    /// A non-interactive run (`codex exec`).
    Exec,
    /// A thread owned by an editor or app client (for example VS Code).
    Client,
    /// A sub-agent spawned by another session.
    SubAgent,
    /// Anything else the CLI reports.
    Other,
}

/// One agent session as the daemon reports it to its clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Unique key across CLIs: `cl:<id>` or `cx:<thread id>`.
    pub key: String,
    pub agent: AgentCli,
    /// Identifier understood by the official CLI (`claude attach <id>`,
    /// `codex resume <id>`).
    pub cli_id: String,
    pub name: String,
    pub cwd: String,
    /// Root folder of the project the session belongs to (worktrees folded).
    pub project_root: String,
    /// Display name of the project (last component of `project_root`).
    pub project: String,
    pub kind: SessionKind,
    pub state: SessionState,
    /// Short model name (`opus`, `luna`), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Reasoning effort, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Role reported by the CLI or derived from the team tool layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Key of the session that launched this one, when known reliably.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Unix time in milliseconds when the session started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<i64>,
    /// Unix time in milliseconds since the session is in its current state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_since_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// Official command line that opens this session in a terminal, or
    /// `None` when the CLI offers no way to attach to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attach: Option<Vec<String>>,
    /// Whether the daemon can stop it with an official command.
    #[serde(default)]
    pub stoppable: bool,
}

impl Session {
    /// Returns the attach command as one shell-like line, for display.
    pub fn attach_line(&self) -> Option<String> {
        self.attach.as_ref().map(|argv| {
            argv.iter()
                .map(|arg| {
                    if arg.is_empty() || arg.contains([' ', '"', '\'', '$']) {
                        format!("'{}'", arg.replace('\'', "'\\''"))
                    } else {
                        arg.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
    }
}

/// Shortens a model identifier to the name shown in the agent pill:
/// `claude-opus-5-5` becomes `opus`, `gpt-6-luna` becomes `luna`.
pub fn short_model(model: &str) -> String {
    let model = model.trim();
    if let Some(rest) = model.strip_prefix("claude-") {
        return rest
            .split('-')
            .find(|part| !part.is_empty() && !part.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(rest)
            .to_string();
    }
    if let Some(rest) = model.strip_prefix("gpt-") {
        // gpt-6-luna, gpt-6.1-sol: keep the part after the version.
        if let Some((_, name)) = rest.split_once('-')
            && !name.is_empty()
        {
            return name.to_string();
        }
    }
    model.to_string()
}
