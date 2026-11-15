//! New sessions launched from the app (ADR-0002).
//!
//! A [`LaunchSpec`] is what the user chose in the «Nueva sesión» box: the
//! agent, its model, effort and permission mode (or sandbox), the working
//! folder and the brief they typed. This module turns it into the exact argv
//! of the official CLI and into the command line shown before launching, and
//! checks the safeguards that do not need I/O. It never writes or completes a
//! brief: the text goes to the CLI byte for byte.

use serde::{Deserialize, Serialize};

use crate::project::tilde;
use crate::session::AgentCli;

/// Models offered for Claude Code (aliases that `claude --model` accepts).
pub const CLAUDE_MODELS: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];
/// Models offered for Codex (`codex -m`).
pub const CODEX_MODELS: [&str; 4] = ["gpt-6-luna", "gpt-6.1-sol", "gpt-6.1-terra", "gpt-6-astra"];
/// Reasoning effort, shared by both CLIs.
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// Claude Code permission modes offered (`--permission-mode`). The modes
/// that skip every prompt are left out on purpose.
pub const CLAUDE_PERMISSION_MODES: [&str; 4] = ["manual", "plan", "acceptEdits", "auto"];
/// Codex sandboxes offered (`-s`). `danger-full-access` is left out.
pub const CODEX_SANDBOXES: [&str; 2] = ["read-only", "workspace-write"];

/// Largest brief accepted, in bytes. Linux limits one argument to 128 KiB.
pub const MAX_BRIEF_BYTES: usize = 100 * 1024;

/// Placeholder shown in the command line while the brief is empty.
pub const BRIEF_PLACEHOLDER: &str = "\"<tu encargo>\"";
/// Placeholder for the session name while the brief is empty.
const NAME_PLACEHOLDER: &str = "<primeras palabras>";

/// Models offered for an agent.
pub fn models(agent: AgentCli) -> &'static [&'static str] {
    match agent {
        AgentCli::ClaudeCode => &CLAUDE_MODELS,
        AgentCli::Codex => &CODEX_MODELS,
    }
}

/// Permission modes (Claude Code) or sandboxes (Codex) offered for an agent.
pub fn access_modes(agent: AgentCli) -> &'static [&'static str] {
    match agent {
        AgentCli::ClaudeCode => &CLAUDE_PERMISSION_MODES,
        AgentCli::Codex => &CODEX_SANDBOXES,
    }
}

/// What the user chose for one launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub agent: AgentCli,
    pub model: String,
    pub effort: String,
    /// Permission mode for Claude Code, sandbox for Codex.
    pub access: String,
    /// Absolute working folder.
    pub cwd: String,
    /// The brief, exactly as the user typed it.
    pub brief: String,
}

/// Why a launch is refused before running anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    /// The brief is empty or only whitespace: the app never writes one.
    EmptyBrief,
    /// The brief cannot travel as one argument (too long or with a NUL).
    BadBrief(String),
    /// A model, effort, mode or folder outside the offered choices.
    InvalidOption(String),
}

impl LaunchError {
    /// Error code used in the protocol.
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyBrief => "empty_brief",
            Self::BadBrief(_) => "bad_brief",
            Self::InvalidOption(_) => "invalid_option",
        }
    }

    /// Message shown to the user.
    pub fn message(&self) -> String {
        match self {
            Self::EmptyBrief => "escribe el encargo antes de lanzar".into(),
            Self::BadBrief(detail) | Self::InvalidOption(detail) => detail.clone(),
        }
    }
}

impl LaunchSpec {
    /// Checks the choices and the brief. Nothing is trimmed or rewritten: a
    /// valid spec is launched as it is.
    pub fn validate(&self) -> Result<(), LaunchError> {
        if self.brief.trim().is_empty() {
            return Err(LaunchError::EmptyBrief);
        }
        if self.brief.contains('\0') {
            return Err(LaunchError::BadBrief(
                "el encargo contiene un carácter nulo".into(),
            ));
        }
        if self.brief.len() > MAX_BRIEF_BYTES {
            return Err(LaunchError::BadBrief(format!(
                "el encargo supera {} KiB",
                MAX_BRIEF_BYTES / 1024
            )));
        }
        let offered = |value: &str, list: &[&str], what: &str| {
            if list.contains(&value) {
                Ok(())
            } else {
                Err(LaunchError::InvalidOption(format!(
                    "{what} no válido: «{value}»"
                )))
            }
        };
        offered(&self.model, models(self.agent), "modelo")?;
        offered(&self.effort, &EFFORTS, "esfuerzo")?;
        offered(
            &self.access,
            access_modes(self.agent),
            match self.agent {
                AgentCli::ClaudeCode => "modo de permisos",
                AgentCli::Codex => "sandbox",
            },
        )?;
        if !is_absolute(&self.cwd) {
            return Err(LaunchError::InvalidOption(format!(
                "la carpeta debe ser una ruta absoluta: «{}»",
                self.cwd
            )));
        }
        Ok(())
    }

    /// Display name given to a Claude Code session (`-n`): the first words
    /// of the brief's first line. Codex names its threads by itself.
    pub fn session_name(&self) -> String {
        session_name(&self.brief)
    }

    /// The argv of the official CLI, program first (`claude` or `codex`).
    /// The brief is the last argument, untouched.
    pub fn argv(&self) -> Vec<String> {
        let mut argv: Vec<String> = match self.agent {
            AgentCli::ClaudeCode => vec![
                "claude".into(),
                "--bg".into(),
                "-n".into(),
                self.session_name(),
                "--model".into(),
                self.model.clone(),
                "--effort".into(),
                self.effort.clone(),
                "--permission-mode".into(),
                self.access.clone(),
            ],
            // `--remote unix://` joins the shared `codex app-server`, so the
            // thread is live there (listed, stoppable) like any other.
            AgentCli::Codex => vec![
                "codex".into(),
                "--remote".into(),
                "unix://".into(),
                "-m".into(),
                self.model.clone(),
                "-c".into(),
                format!("model_reasoning_effort={}", self.effort),
                "-s".into(),
                self.access.clone(),
            ],
        };
        // A brief that starts with a dash must not be read as an option.
        if self.brief.starts_with('-') {
            argv.push("--".into());
        }
        argv.push(self.brief.clone());
        argv
    }

    /// The exact command shown before launching: `cd <folder> && <argv>`,
    /// quoted for a POSIX shell, with the home folder written as `~`.
    pub fn command_line(&self, home: Option<&str>) -> String {
        let argv = self.argv();
        let mut line = format!("cd {} && ", quote_path(&self.cwd, home));
        line.push_str(&join_argv(&argv));
        line
    }

    /// The command line with a placeholder instead of the brief, for the box
    /// while it is still empty.
    pub fn command_preview(&self, home: Option<&str>) -> String {
        if !self.brief.trim().is_empty() {
            return self.command_line(home);
        }
        let mut argv = self.argv();
        argv.pop();
        if self.agent == AgentCli::ClaudeCode {
            argv[3] = NAME_PLACEHOLDER.into();
        }
        let mut line = format!("cd {} && ", quote_path(&self.cwd, home));
        line.push_str(&join_argv(&argv));
        line.push(' ');
        line.push_str(BRIEF_PLACEHOLDER);
        line
    }

    /// A key that identifies "the same launch" to detect double clicks.
    pub fn fingerprint(&self) -> String {
        format!("{}\u{1f}{}\u{1f}{}", self.agent.tag(), self.cwd, self.brief)
    }
}

/// Name for `claude -n`: the first six words of the first non-empty line,
/// at most 48 characters.
pub fn session_name(brief: &str) -> String {
    let line = brief
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let mut name = String::new();
    for word in line.split_whitespace().take(6) {
        let candidate = if name.is_empty() {
            word.to_string()
        } else {
            format!("{name} {word}")
        };
        if candidate.chars().count() > 48 {
            if name.is_empty() {
                name = word.chars().take(48).collect();
            }
            break;
        }
        name = candidate;
    }
    let name = name.trim_end_matches([',', '.', ':', ';']).to_string();
    if name.is_empty() {
        "nueva sesión".into()
    } else {
        name
    }
}

/// Reads the short id that `claude --bg` prints
/// (`backgrounded · f3c0ab80 · <name>`).
pub fn parse_claude_bg_output(stdout: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        line.split(|c: char| c.is_whitespace() || c == '·')
            .find(|token| token.len() == 8 && token.chars().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_string)
    })
}

/// Quotes one argument for a POSIX shell. Plain words stay bare; text with
/// spaces goes in double quotes when that is exact, otherwise in single
/// quotes.
pub fn shell_quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-')
        });
    if plain {
        return arg.to_string();
    }
    let double_safe = !arg.contains(['"', '$', '`', '\\', '!']);
    if double_safe {
        format!("\"{arg}\"")
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

fn join_argv(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Quotes a folder for `cd`, keeping a leading `~` outside the quotes so the
/// shell still expands it.
fn quote_path(path: &str, home: Option<&str>) -> String {
    let shown = tilde(path, home);
    if shown == "~" {
        return shown;
    }
    match shown.strip_prefix("~/") {
        Some(rest) => format!("~/{}", shell_quote(rest)),
        None => shell_quote(&shown),
    }
}

fn is_absolute(path: &str) -> bool {
    path.starts_with('/')
        || (path.len() >= 3
            && path.as_bytes()[0].is_ascii_alphabetic()
            && path.as_bytes()[1] == b':'
            && matches!(path.as_bytes()[2], b'\\' | b'/'))
}

/// Default choices of the «Nueva sesión» box, edited in `[*]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchDefaults {
    pub agent: AgentCli,
    pub claude: AgentDefaults,
    pub codex: AgentDefaults,
}

/// Default model, effort and access of one agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDefaults {
    pub model: String,
    pub effort: String,
    pub access: String,
}

impl Default for LaunchDefaults {
    fn default() -> Self {
        LaunchDefaults {
            agent: AgentCli::ClaudeCode,
            claude: AgentDefaults {
                model: "opus".into(),
                effort: "high".into(),
                access: "manual".into(),
            },
            codex: AgentDefaults {
                model: "gpt-6-luna".into(),
                effort: "high".into(),
                access: "workspace-write".into(),
            },
        }
    }
}

impl LaunchDefaults {
    /// Parses saved defaults; anything unknown falls back to the built-in
    /// value, so an old or hand-edited file never breaks the box.
    pub fn parse(json: &str) -> LaunchDefaults {
        let mut defaults: LaunchDefaults = serde_json::from_str(json).unwrap_or_default();
        defaults.sanitize();
        defaults
    }

    /// Replaces values that are not offered any more by the built-in ones.
    pub fn sanitize(&mut self) {
        let builtin = LaunchDefaults::default();
        for agent in AgentCli::ALL {
            let fallback = builtin.of(agent).clone();
            let current = self.of_mut(agent);
            if !models(agent).contains(&current.model.as_str()) {
                current.model = fallback.model;
            }
            if !EFFORTS.contains(&current.effort.as_str()) {
                current.effort = fallback.effort;
            }
            if !access_modes(agent).contains(&current.access.as_str()) {
                current.access = fallback.access;
            }
        }
    }

    pub fn of(&self, agent: AgentCli) -> &AgentDefaults {
        match agent {
            AgentCli::ClaudeCode => &self.claude,
            AgentCli::Codex => &self.codex,
        }
    }

    pub fn of_mut(&mut self, agent: AgentCli) -> &mut AgentDefaults {
        match agent {
            AgentCli::ClaudeCode => &mut self.claude,
            AgentCli::Codex => &mut self.codex,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("defaults always serialize")
    }
}

/// What is known of a Codex TUI launched by the daemon whose thread has not
/// been identified yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchedTui {
    pub cwd: String,
    pub brief: String,
    /// Unix time in milliseconds of the launch.
    pub launched_ms: i64,
}

/// Finds the thread a launched Codex TUI started among the Codex sessions:
/// one that is not in `taken` (it existed at launch time or already has a
/// terminal), works in the same project and did not start long before the
/// launch. When several qualify, the one named after the brief's first word
/// wins, then the oldest.
pub fn pick_launched_thread<'a>(
    launched: &LaunchedTui,
    sessions: &'a [crate::session::Session],
    taken: &dyn Fn(&str) -> bool,
) -> Option<&'a crate::session::Session> {
    let root = crate::project::project_root(&launched.cwd);
    let mut candidates: Vec<&crate::session::Session> = sessions
        .iter()
        .filter(|s| s.agent == AgentCli::Codex && !taken(&s.key))
        .filter(|s| s.cwd == launched.cwd || s.project_root == root)
        .filter(|s| {
            s.started_at_ms
                .is_none_or(|started| started >= launched.launched_ms - 60_000)
        })
        .collect();
    candidates.sort_by_key(|s| s.started_at_ms.unwrap_or(i64::MAX));
    let first_word = launched.brief.split_whitespace().next().unwrap_or("");
    candidates
        .iter()
        .find(|s| !first_word.is_empty() && s.name.starts_with(first_word))
        .or_else(|| candidates.first())
        .copied()
}
