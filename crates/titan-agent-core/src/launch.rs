//! New sessions launched from the app, with the safeguards of ADR-0002.
//!
//! A [`LaunchSpec`] holds what the user chose in the "new session" box. This
//! module turns it into the exact argv of the official CLI and the command
//! line shown to the user before launching; the daemon only runs a spec whose
//! command line is the one the user saw. Nothing here writes or completes the
//! brief: it goes to the CLI exactly as typed.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::project::{project_name, project_root};
use crate::session::{AgentCli, Session, SessionKind, SessionState, short_model};

/// Largest brief accepted, in bytes.
pub const MAX_BRIEF_BYTES: usize = 32_000;

/// Characters of the brief's first line kept in a Claude Code session name.
const NAME_CHARS: usize = 40;

/// What the user chose in the "new session" box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchSpec {
    pub agent: AgentCli,
    /// Working folder, an absolute path.
    pub cwd: String,
    pub model: String,
    pub effort: String,
    /// Claude Code permission mode or Codex sandbox.
    pub permission: String,
    /// The brief, exactly as the user wrote it.
    pub brief: String,
}

/// Why a [`LaunchSpec`] cannot be launched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    EmptyBrief,
    BriefTooLong,
    BadBrief,
    BadFolder,
    BadOption { field: &'static str, value: String },
}

impl LaunchError {
    /// Stable code used in protocol errors.
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyBrief => "empty_brief",
            Self::BriefTooLong => "brief_too_long",
            Self::BadBrief => "bad_brief",
            Self::BadFolder => "bad_folder",
            Self::BadOption { .. } => "bad_option",
        }
    }
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBrief => f.write_str("escribe el encargo antes de lanzar"),
            Self::BriefTooLong => write!(f, "el encargo pasa de {MAX_BRIEF_BYTES} bytes"),
            Self::BadBrief => f.write_str("el encargo contiene un carácter nulo"),
            Self::BadFolder => {
                f.write_str("la carpeta de trabajo debe ser una ruta absoluta sin . ni ..")
            }
            Self::BadOption { field, value } => write!(f, "{field} no válido: «{value}»"),
        }
    }
}

impl std::error::Error for LaunchError {}

/// Models offered in the "new session" box. Any other valid model name can
/// still be set in the defaults file.
pub fn models(agent: AgentCli) -> &'static [&'static str] {
    match agent {
        AgentCli::ClaudeCode => &["opus", "sonnet", "haiku", "fable"],
        AgentCli::Codex => &["gpt-6-luna", "gpt-6.1-sol", "gpt-6.1-terra"],
    }
}

/// Default model, effort and permission of one CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDefaults {
    pub model: String,
    pub effort: String,
    pub permission: String,
}

/// Values the "new session" box starts with, configured from `[*]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchDefaults {
    pub agent: AgentCli,
    pub claude_code: AgentDefaults,
    pub codex: AgentDefaults,
}

impl Default for LaunchDefaults {
    fn default() -> LaunchDefaults {
        LaunchDefaults {
            agent: AgentCli::ClaudeCode,
            claude_code: AgentDefaults {
                model: "opus".into(),
                effort: "high".into(),
                permission: "manual".into(),
            },
            codex: AgentDefaults {
                model: "gpt-6-luna".into(),
                effort: "medium".into(),
                permission: "workspace-write".into(),
            },
        }
    }
}

impl LaunchDefaults {
    /// Reads the defaults file, keeping the built-in value of anything
    /// missing or not accepted by [`LaunchSpec::validate`].
    pub fn parse(text: &str) -> LaunchDefaults {
        let mut defaults: LaunchDefaults = serde_json::from_str(text).unwrap_or_default();
        let fallback = LaunchDefaults::default();
        for agent in AgentCli::ALL {
            let good = fallback.get(agent).clone();
            let value = defaults.get_mut(agent);
            if !is_model(&value.model) {
                value.model = good.model;
            }
            if !efforts(agent).contains(&value.effort.as_str()) {
                value.effort = good.effort;
            }
            if !permissions(agent).contains(&value.permission.as_str()) {
                value.permission = good.permission;
            }
        }
        defaults
    }

    pub fn get(&self, agent: AgentCli) -> &AgentDefaults {
        match agent {
            AgentCli::ClaudeCode => &self.claude_code,
            AgentCli::Codex => &self.codex,
        }
    }

    pub fn get_mut(&mut self, agent: AgentCli) -> &mut AgentDefaults {
        match agent {
            AgentCli::ClaudeCode => &mut self.claude_code,
            AgentCli::Codex => &mut self.codex,
        }
    }
}

/// Effort levels offered for each CLI.
pub fn efforts(agent: AgentCli) -> &'static [&'static str] {
    match agent {
        AgentCli::ClaudeCode => &["low", "medium", "high", "xhigh", "max"],
        AgentCli::Codex => &["low", "medium", "high", "xhigh"],
    }
}

/// Permission modes (Claude Code) or sandboxes (Codex) accepted. The modes
/// that skip permissions or the sandbox are left out on purpose.
pub fn permissions(agent: AgentCli) -> &'static [&'static str] {
    match agent {
        AgentCli::ClaudeCode => &["manual", "plan", "acceptEdits", "auto"],
        AgentCli::Codex => &["read-only", "workspace-write"],
    }
}

impl LaunchSpec {
    /// Checks the safeguards that do not need I/O. The daemon also checks
    /// that the folder exists.
    pub fn validate(&self) -> Result<(), LaunchError> {
        if self.brief.trim().is_empty() {
            return Err(LaunchError::EmptyBrief);
        }
        if self.brief.len() > MAX_BRIEF_BYTES {
            return Err(LaunchError::BriefTooLong);
        }
        if self.brief.contains('\0') {
            return Err(LaunchError::BadBrief);
        }
        // `cd` and the daemon resolve `..` after a link differently, so the
        // folder must be absolute and plain.
        if !is_absolute(&self.cwd)
            || self
                .cwd
                .split(SEPARATORS)
                .any(|part| part == "." || part == "..")
        {
            return Err(LaunchError::BadFolder);
        }
        if !is_model(&self.model) {
            return Err(self.bad("modelo", &self.model));
        }
        if !efforts(self.agent).contains(&self.effort.as_str()) {
            return Err(self.bad("esfuerzo", &self.effort));
        }
        if !permissions(self.agent).contains(&self.permission.as_str()) {
            let field = match self.agent {
                AgentCli::ClaudeCode => "modo de permisos",
                AgentCli::Codex => "sandbox",
            };
            return Err(self.bad(field, &self.permission));
        }
        Ok(())
    }

    fn bad(&self, field: &'static str, value: &str) -> LaunchError {
        LaunchError::BadOption {
            field,
            value: value.to_string(),
        }
    }

    /// Display name for `claude --bg -n`: the folder and the start of the
    /// brief's first non-empty line.
    pub fn session_name(&self) -> String {
        let folder = folder_name(&self.cwd);
        let first = self
            .brief
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("");
        let mut start: String = first.chars().take(NAME_CHARS).collect();
        if first.chars().count() > NAME_CHARS {
            start.push('…');
        }
        format!("{folder}: {start}")
    }

    /// The exact argv of the official CLI. The brief is the last argument,
    /// after `--`, untouched.
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
                self.permission.clone(),
            ],
            // `--remote unix://` joins the shared `codex app-server`, so the
            // thread is live there (listed, stoppable) while the TUI runs.
            AgentCli::Codex => vec![
                "codex".into(),
                "--remote".into(),
                "unix://".into(),
                "-m".into(),
                self.model.clone(),
                "-c".into(),
                format!("model_reasoning_effort={}", self.effort),
                "-s".into(),
                self.permission.clone(),
            ],
        };
        argv.push("--".into());
        argv.push(self.brief.clone());
        argv
    }

    /// The line shown to the user before launching. `sh -c` on it runs
    /// exactly [`LaunchSpec::argv`] in [`LaunchSpec::cwd`].
    pub fn command_line(&self) -> String {
        let mut line = format!("cd {} &&", shell_quote(&self.cwd));
        for arg in self.argv() {
            line.push(' ');
            line.push_str(&shell_quote(&arg));
        }
        line
    }

    /// [`LaunchSpec::command_line`] for display, with `home` at the start
    /// of the folder written as `~` (as the tree shows it). The shell reads
    /// it the same way while `HOME` is `home`; what the daemon checks is
    /// still [`LaunchSpec::command_line`].
    pub fn display_line(&self, home: Option<&str>) -> String {
        let line = self.command_line();
        let folder = crate::project::tilde(&self.cwd, home);
        let Some(rest) = folder.strip_prefix('~') else {
            return line;
        };
        let cd = match rest.strip_prefix('/') {
            Some(rest) if !rest.is_empty() => format!("cd ~/{}", shell_quote(rest)),
            _ => "cd ~".to_string(),
        };
        let quoted = format!("cd {}", shell_quote(&self.cwd));
        line.replacen(&quoted, &cd, 1)
    }
}

impl LaunchSpec {
    /// The session shown in the tree from the click until the CLI reports
    /// the real one. It cannot be attached or stopped yet.
    pub fn provisional_session(&self, key: &str, launched_at_ms: i64) -> Session {
        let root = project_root(&self.cwd);
        Session {
            key: key.to_string(),
            agent: self.agent,
            cli_id: String::new(),
            name: self.session_name(),
            cwd: self.cwd.clone(),
            project: project_name(&root),
            project_root: root,
            kind: match self.agent {
                AgentCli::ClaudeCode => SessionKind::Background,
                AgentCli::Codex => SessionKind::Interactive,
            },
            state: SessionState::Working,
            model: Some(short_model(&self.model)),
            effort: Some(self.effort.clone()),
            role: None,
            parent: None,
            started_at_ms: Some(launched_at_ms),
            state_since_ms: Some(launched_at_ms),
            pid: None,
            attach: None,
            stoppable: false,
            launched_here: true,
        }
    }

    /// Whether `session`, which was not known when this spec was launched at
    /// `launched_at_ms`, is the one it created: same CLI, same folder and
    /// started around the launch or later. Used for Codex, whose TUI does not
    /// print the new thread id.
    pub fn matches_new_session(&self, session: &Session, launched_at_ms: i64) -> bool {
        session.agent == self.agent
            && same_folder(&session.cwd, &self.cwd)
            && session
                .started_at_ms
                .is_none_or(|started| started >= launched_at_ms - 5_000)
    }
}

fn same_folder(a: &str, b: &str) -> bool {
    a.trim_end_matches(['/', '\\']) == b.trim_end_matches(['/', '\\'])
}

/// Quotes one word for a POSIX shell, leaving plain words bare.
pub fn shell_quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./=:,@%+-".contains(c));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

/// Path separators: `\\` only counts on Windows.
const SEPARATORS: &[char] = if cfg!(windows) { &['/', '\\'] } else { &['/'] };

/// Last component of a folder path, ignoring trailing separators.
pub fn folder_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches(SEPARATORS);
    let name = trimmed.rsplit(SEPARATORS).next().unwrap_or("");
    if name.is_empty() { path } else { name }
}

fn is_absolute(path: &str) -> bool {
    if path.starts_with('/') {
        return true;
    }
    // Windows: `C:\…` or `C:/…`.
    let bytes = path.as_bytes();
    cfg!(windows)
        && bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn is_model(model: &str) -> bool {
    !model.is_empty()
        && !model.starts_with('-')
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:[]-".contains(c))
}

/// Parses the id that `claude --bg` prints (`backgrounded · <id> · <name>`).
pub fn parse_backgrounded_id(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("backgrounded")?;
        let id = rest
            .trim_start()
            .strip_prefix('·')?
            .split('·')
            .next()?
            .trim();
        (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .then(|| id.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(agent: AgentCli, brief: &str) -> LaunchSpec {
        LaunchSpec {
            agent,
            cwd: "/srv/demo/kite".into(),
            model: if agent == AgentCli::Codex {
                "gpt-6-luna".into()
            } else {
                "haiku".into()
            },
            effort: "low".into(),
            permission: permissions(agent)[1].into(),
            brief: brief.into(),
        }
    }

    #[test]
    fn claude_argv_and_line() {
        let s = spec(AgentCli::ClaudeCode, "hola");
        assert_eq!(
            s.command_line(),
            "cd /srv/demo/kite && claude --bg -n 'kite: hola' --model haiku --effort low \
             --permission-mode plan -- hola"
        );
    }

    #[test]
    fn the_shown_line_writes_home_as_a_tilde() {
        let mut s = spec(AgentCli::ClaudeCode, "hola");
        assert_eq!(s.display_line(Some("/srv/demo")), {
            "cd ~/kite && claude --bg -n 'kite: hola' --model haiku --effort low \
             --permission-mode plan -- hola"
        });
        assert_eq!(s.display_line(Some("/srv/dem")), s.command_line());
        assert_eq!(s.display_line(None), s.command_line());
        s.cwd = "/srv/demo/my kite".into();
        assert!(
            s.display_line(Some("/srv/demo/"))
                .starts_with("cd ~/'my kite' && ")
        );
        s.cwd = "/srv/demo".into();
        assert!(s.display_line(Some("/srv/demo")).starts_with("cd ~ && "));
    }

    #[test]
    fn launched_here_is_marked_and_only_serialized_when_true() {
        let provisional = spec(AgentCli::Codex, "hola").provisional_session("launch:1", 0);
        assert!(provisional.launched_here);
        let value = serde_json::to_value(&provisional).unwrap();
        assert_eq!(value["launched_here"], true);

        let mut other = provisional.clone();
        other.launched_here = false;
        let mut value = serde_json::to_value(&other).unwrap();
        assert!(value.get("launched_here").is_none());
        // An older daemon does not send the field at all.
        value.as_object_mut().unwrap().remove("launched_here");
        let back: Session = serde_json::from_value(value).unwrap();
        assert!(!back.launched_here);
    }

    #[test]
    fn codex_argv() {
        let s = spec(AgentCli::Codex, "-x it's");
        assert_eq!(
            s.argv(),
            [
                "codex",
                "--remote",
                "unix://",
                "-m",
                "gpt-6-luna",
                "-c",
                "model_reasoning_effort=low",
                "-s",
                "workspace-write",
                "--",
                "-x it's"
            ]
        );
        assert!(s.command_line().ends_with(" -- '-x it'\\''s'"));
    }

    #[test]
    fn name_uses_first_line_and_cuts_by_chars() {
        let mut s = spec(
            AgentCli::ClaudeCode,
            "\n  ñandú ñandú ñandú ñandú ñandú ñandú ñandú ñandú\nmás",
        );
        assert_eq!(
            s.session_name(),
            "kite: ñandú ñandú ñandú ñandú ñandú ñandú ñand…"
        );
        s.cwd = "/srv/demo/kite/".into();
        s.brief = "corto".into();
        assert_eq!(s.session_name(), "kite: corto");
    }

    #[test]
    fn validation_order_and_codes() {
        let ok = spec(AgentCli::ClaudeCode, "hola");
        assert_eq!(ok.validate(), Ok(()));
        let check = |f: &dyn Fn(&mut LaunchSpec), code: &str| {
            let mut s = ok.clone();
            f(&mut s);
            assert_eq!(s.validate().unwrap_err().code(), code, "{s:?}");
        };
        check(&|s| s.brief = " \n\t".into(), "empty_brief");
        check(
            &|s| s.brief = "x".repeat(MAX_BRIEF_BYTES + 1),
            "brief_too_long",
        );
        check(&|s| s.brief = "a\0b".into(), "bad_brief");
        check(&|s| s.cwd = "relative/dir".into(), "bad_folder");
        check(&|s| s.cwd = "/srv/demo/../kite".into(), "bad_folder");
        check(&|s| s.cwd = "/srv/./kite".into(), "bad_folder");
        check(&|s| s.model = "--dangerously".into(), "bad_option");
        check(&|s| s.model = "opus 4".into(), "bad_option");
        check(&|s| s.effort = "turbo".into(), "bad_option");
        check(&|s| s.permission = "bypassPermissions".into(), "bad_option");
        check(&|s| s.permission = "read-only".into(), "bad_option");
        // Empty brief wins over a bad folder.
        check(
            &|s| {
                s.brief = String::new();
                s.cwd = String::new()
            },
            "empty_brief",
        );
        let mut s = ok.clone();
        s.model = "opus[1m]".into();
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn parses_backgrounded_line() {
        let out = "backgrounded · 4ae6f6cf · kite: hola\n  claude agents   list sessions\n";
        assert_eq!(parse_backgrounded_id(out).as_deref(), Some("4ae6f6cf"));
        assert_eq!(parse_backgrounded_id("something else"), None);
    }

    #[test]
    fn provisional_session_and_matching() {
        let s = spec(AgentCli::Codex, "hola");
        let p = s.provisional_session("launch:3", 10_000);
        assert_eq!(p.key, "launch:3");
        assert_eq!(p.project, "kite");
        assert_eq!(p.model.as_deref(), Some("luna"));
        assert!(p.attach.is_none() && !p.stoppable);
        let mut real = p.clone();
        real.key = "cx:t1".into();
        real.cwd = "/srv/demo/kite/".into();
        real.started_at_ms = Some(9_000);
        assert!(s.matches_new_session(&real, 10_000));
        real.started_at_ms = Some(1_000);
        assert!(!s.matches_new_session(&real, 10_000));
        real.started_at_ms = None;
        real.cwd = "/srv/demo/other".into();
        assert!(!s.matches_new_session(&real, 10_000));
    }

    #[test]
    fn defaults_fall_back_per_field() {
        assert_eq!(LaunchDefaults::parse("no json"), LaunchDefaults::default());
        let d = LaunchDefaults::parse(
            r#"{"agent":"codex","codex":{"model":"gpt-6.1-sol","effort":"max","permission":"danger-full-access"}}"#,
        );
        assert_eq!(d.agent, AgentCli::Codex);
        assert_eq!(d.codex.model, "gpt-6.1-sol");
        assert_eq!(d.codex.effort, "medium");
        assert_eq!(d.codex.permission, "workspace-write");
        assert_eq!(d.claude_code, LaunchDefaults::default().claude_code);
        let text = serde_json::to_string(&d).unwrap();
        assert_eq!(LaunchDefaults::parse(&text), d);
        for agent in AgentCli::ALL {
            let v = LaunchDefaults::default().get(agent).clone();
            let spec = LaunchSpec {
                agent,
                cwd: "/x".into(),
                model: v.model,
                effort: v.effort,
                permission: v.permission,
                brief: "x".into(),
            };
            assert_eq!(spec.validate(), Ok(()));
            assert!(models(agent).iter().all(|m| is_model(m)));
        }
    }

    #[test]
    fn quoting() {
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(
            shell_quote("model_reasoning_effort=low"),
            "model_reasoning_effort=low"
        );
        assert_eq!(folder_name("/"), "/");
        if cfg!(unix) {
            assert_eq!(folder_name("/srv/demo/foo\\bar"), "foo\\bar");
        }
    }
}
