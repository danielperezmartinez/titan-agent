use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// The official CLI used to run an agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentCli {
    ClaudeCode,
    Codex,
}
impl Serialize for AgentCli {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.command())
    }
}
impl<'de> Deserialize<'de> for AgentCli {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = String::deserialize(d)?;
        Self::from_command(&v).ok_or_else(|| serde::de::Error::custom("unknown CLI"))
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
impl Serialize for SessionState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match self {
            Self::Working => "working",
            Self::NeedsYou => "needs_you",
            Self::Waiting => "waiting",
            Self::Finished { failed: false } => "finished",
            Self::Finished { failed: true } => "failed",
        })
    }
}
impl<'de> Deserialize<'de> for SessionState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "working" => Ok(Self::Working),
            "needs_you" => Ok(Self::NeedsYou),
            "waiting" => Ok(Self::Waiting),
            "finished" => Ok(Self::Finished { failed: false }),
            "failed" => Ok(Self::Finished { failed: true }),
            _ => Err(serde::de::Error::custom("unknown session state")),
        }
    }
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
