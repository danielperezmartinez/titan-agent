//! `.agents/team.toml`: the team of a project (ADR-0005).
//!
//! Nothing about the team is fixed in code: roles, CLI, model, effort,
//! sandbox, prompts, paths, flow and the review limit live in this file. It is
//! parsed strictly (an unknown key or value is an error that names it), so an
//! agent that sets up the team of another project gets a clear message
//! instead of a silently ignored setting. The documented format is
//! `crates/titan-team/FORMAT.md` (`titan-team doc`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Version of the `team.toml` format understood by this build.
pub const CONFIG_VERSION: u32 = 1;

/// Sandboxes a role may use. `danger-full-access` is never accepted.
pub const SANDBOXES: [&str; 2] = ["read-only", "workspace-write"];

/// Reasoning efforts accepted for a role.
pub const EFFORTS: [&str; 6] = ["none", "minimal", "low", "medium", "high", "xhigh"];

/// CLIs a role may name. Only `codex` roles are launched by the tool; a
/// `claude` role is the director (the Claude Code session that runs it).
pub const CLIS: [&str; 2] = ["codex", "claude"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamConfig {
    pub version: u32,
    #[serde(default)]
    pub name: Option<String>,
    /// Where the tool keeps the state of each task, relative to the folder
    /// that holds `.agents/`.
    #[serde(default = "default_state_dir")]
    pub state_dir: String,
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub codex: CodexConfig,
    #[serde(default)]
    pub flow: FlowConfig,
    #[serde(default)]
    pub roles: BTreeMap<String, RoleConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(default = "default_repo")]
    pub repo: String,
    #[serde(default = "default_base")]
    pub base: String,
    #[serde(default = "default_worktrees")]
    pub worktrees_dir: String,
    #[serde(default = "default_branch_prefix")]
    pub branch_prefix: String,
    #[serde(default)]
    pub test_cmd: Option<String>,
    #[serde(default = "default_test_timeout")]
    pub test_timeout_s: u64,
    #[serde(default)]
    pub autocommit: bool,
    /// Left out of every task worktree with a sparse checkout.
    #[serde(default)]
    pub hidden_paths: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            repo: default_repo(),
            base: default_base(),
            worktrees_dir: default_worktrees(),
            branch_prefix: default_branch_prefix(),
            test_cmd: None,
            test_timeout_s: default_test_timeout(),
            autocommit: false,
            hidden_paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexConfig {
    /// `-c key=value` options for every `codex exec` call.
    #[serde(default)]
    pub config: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowConfig {
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub review_role: Option<String>,
    #[serde(default)]
    pub fix_role: Option<String>,
    #[serde(default)]
    pub max_review_rounds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleConfig {
    pub cli: String,
    pub model: String,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub sandbox: Option<String>,
    #[serde(default)]
    pub prompt_file: Option<String>,
    #[serde(default)]
    pub output_schema: Option<String>,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub forbidden_paths: Vec<String>,
    #[serde(default = "default_role_timeout")]
    pub timeout_s: u64,
    #[serde(default)]
    pub purpose: Option<String>,
    /// Short name shown in the monitor (`tests`, `revisor`); the role name
    /// when unset.
    #[serde(default)]
    pub label: Option<String>,
    /// False for the director: the tool never launches it.
    #[serde(default = "default_true")]
    pub launch: bool,
    /// Extra `-c` options of this role, after `[codex].config`.
    #[serde(default)]
    pub config: Vec<String>,
}

impl RoleConfig {
    /// The sandbox of a Codex role (`read-only` when unset).
    pub fn sandbox(&self) -> &str {
        self.sandbox.as_deref().unwrap_or("read-only")
    }

    /// Whether the tool may run this role (`launch` and a Codex CLI).
    pub fn launchable(&self) -> bool {
        self.launch && self.cli == "codex"
    }
}

fn default_state_dir() -> String {
    ".agents/state".into()
}
fn default_repo() -> String {
    ".".into()
}
fn default_base() -> String {
    "main".into()
}
fn default_worktrees() -> String {
    ".agents/worktrees".into()
}
fn default_branch_prefix() -> String {
    "encargo/".into()
}
fn default_test_timeout() -> u64 {
    300
}
fn default_role_timeout() -> u64 {
    900
}
fn default_true() -> bool {
    true
}

impl TeamConfig {
    /// Parses and validates the text of a `team.toml`.
    pub fn parse(text: &str) -> Result<TeamConfig, String> {
        let config: TeamConfig = toml::from_str(text).map_err(|error| {
            let message = error.message().trim().to_string();
            match error.span() {
                Some(span) => {
                    let line = text[..span.start.min(text.len())].lines().count().max(1);
                    format!("team.toml, línea {line}: {message}")
                }
                None => format!("team.toml: {message}"),
            }
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Checks the values serde cannot: versions, vocabularies and references
    /// between sections.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != CONFIG_VERSION {
            return Err(format!(
                "team.toml: version = {} no es válida; esta herramienta lee la versión {CONFIG_VERSION}",
                self.version
            ));
        }
        if self.roles.is_empty() {
            return Err("team.toml: falta al menos un rol en [roles.<nombre>]".into());
        }
        for (name, role) in &self.roles {
            if !is_identifier(name) {
                return Err(format!(
                    "team.toml: el rol «{name}» debe usar solo minúsculas, cifras, - y _"
                ));
            }
            let field = |field: &str| format!("team.toml: [roles.{name}].{field}");
            if !CLIS.contains(&role.cli.as_str()) {
                return Err(format!(
                    "{} = \"{}\" no es válido; usa uno de: {}",
                    field("cli"),
                    role.cli,
                    CLIS.join(", ")
                ));
            }
            if role.model.trim().is_empty() {
                return Err(format!("{} no puede ir vacío", field("model")));
            }
            if let Some(effort) = &role.effort
                && !EFFORTS.contains(&effort.as_str())
            {
                return Err(format!(
                    "{} = \"{effort}\" no es válido; usa uno de: {}",
                    field("effort"),
                    EFFORTS.join(", ")
                ));
            }
            if let Some(sandbox) = &role.sandbox
                && !SANDBOXES.contains(&sandbox.as_str())
            {
                return Err(format!(
                    "{} = \"{sandbox}\" no es válido; usa uno de: {}",
                    field("sandbox"),
                    SANDBOXES.join(", ")
                ));
            }
            if role.cli == "claude" && role.launch {
                return Err(format!(
                    "{}: un rol de Claude Code es el director y no se lanza; pon launch = false",
                    field("cli")
                ));
            }
            if role.launchable() && role.prompt_file.is_none() {
                return Err(format!(
                    "{} es obligatorio en un rol que se lanza",
                    field("prompt_file")
                ));
            }
            if role.timeout_s == 0 {
                return Err(format!("{} debe ser mayor que 0", field("timeout_s")));
            }
        }
        let known = |role: &str| self.roles.contains_key(role);
        for step in &self.flow.steps {
            if !known(step) {
                return Err(format!(
                    "team.toml: [flow].steps nombra el rol «{step}», que no existe"
                ));
            }
        }
        for (key, value) in [
            ("review_role", &self.flow.review_role),
            ("fix_role", &self.flow.fix_role),
        ] {
            if let Some(role) = value
                && !known(role)
            {
                return Err(format!(
                    "team.toml: [flow].{key} nombra el rol «{role}», que no existe"
                ));
            }
        }
        if self.project.test_timeout_s == 0 {
            return Err("team.toml: [project].test_timeout_s debe ser mayor que 0".into());
        }
        if self.state_dir.trim().is_empty() {
            return Err("team.toml: state_dir no puede ir vacío".into());
        }
        Ok(())
    }

    /// The director: the first role that is not launched, if any.
    pub fn director(&self) -> Option<(&String, &RoleConfig)> {
        self.roles.iter().find(|(_, role)| !role.launch)
    }

    /// Launchable roles in display order: the flow steps first, then the
    /// rest by name.
    pub fn ordered_roles(&self) -> Vec<&String> {
        let mut names: Vec<&String> = self
            .flow
            .steps
            .iter()
            .filter_map(|step| self.roles.get_key_value(step).map(|(k, _)| k))
            .collect();
        for name in self.roles.keys() {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names.retain(|name| self.roles[*name].launch);
        names
    }
}

/// Task and role names: lower case letters, digits, `-` and `_`.
pub fn is_identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Matches a path against a role pattern: shell wildcards (`*` within one
/// component, `?`, `**` across components), and a trailing `/` covers
/// everything below that folder.
pub fn path_matches(path: &str, pattern: &str) -> bool {
    let pattern = pattern.trim_start_matches('/');
    if let Some(folder) = pattern.strip_suffix('/') {
        return glob(&format!("{folder}/**"), path);
    }
    glob(pattern, path)
}

fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_at(&p, &t)
}

fn glob_at(p: &[char], t: &[char]) -> bool {
    match p.first() {
        None => t.is_empty(),
        Some('*') if p.get(1) == Some(&'*') => {
            let rest = &p[2..];
            (0..=t.len()).any(|i| glob_at(rest, &t[i..]))
        }
        Some('*') => {
            let rest = &p[1..];
            let mut i = 0;
            loop {
                if glob_at(rest, &t[i..]) {
                    return true;
                }
                if i == t.len() || t[i] == '/' {
                    return false;
                }
                i += 1;
            }
        }
        Some('?') => !t.is_empty() && t[0] != '/' && glob_at(&p[1..], &t[1..]),
        Some(c) => t.first() == Some(c) && glob_at(&p[1..], &t[1..]),
    }
}

/// Warnings for the files a role changed outside its allowed paths or inside
/// its forbidden ones.
pub fn path_warnings(role: &RoleConfig, changed: &[String]) -> Vec<String> {
    let mut warnings = Vec::new();
    for file in changed {
        if !role.allowed_paths.is_empty()
            && !role.allowed_paths.iter().any(|p| path_matches(file, p))
        {
            warnings.push(format!("Fichero fuera de allowed_paths: {file}"));
        }
        if role.forbidden_paths.iter().any(|p| path_matches(file, p)) {
            warnings.push(format!("Fichero dentro de forbidden_paths: {file}"));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
version = 1
[flow]
steps = ["impl"]
review_role = "impl"
[roles.director]
cli = "claude"
model = "opus"
launch = false
[roles.impl]
cli = "codex"
model = "gpt-6-luna"
effort = "medium"
sandbox = "workspace-write"
prompt_file = ".agents/prompts/impl.md"
"#;

    #[test]
    fn the_repo_team_parses() {
        let text = include_str!("../../../../.agents/team.toml");
        let config = TeamConfig::parse(text).unwrap();
        assert_eq!(config.flow.steps, ["disenador_tests", "revisor"]);
        assert_eq!(config.flow.max_review_rounds, 1);
        assert_eq!(config.director().unwrap().0, "director");
        assert_eq!(
            config.ordered_roles(),
            ["disenador_tests", "revisor", "mecanico"]
        );
        assert_eq!(config.roles["revisor"].sandbox(), "read-only");
    }

    #[test]
    fn the_starter_team_of_init_parses() {
        let text = include_str!("../../../titan-team/team.example.toml");
        let config = TeamConfig::parse(text).unwrap();
        assert_eq!(config.roles["revisor"].label.as_deref(), Some("revisor"));
    }

    #[test]
    fn defaults_fill_what_is_left_out() {
        let config = TeamConfig::parse(MINIMAL).unwrap();
        assert_eq!(config.state_dir, ".agents/state");
        assert_eq!(config.project.base, "main");
        assert_eq!(config.project.branch_prefix, "encargo/");
        assert_eq!(config.roles["impl"].timeout_s, 900);
        assert!(config.roles["impl"].launchable());
        assert!(!config.roles["director"].launchable());
    }

    #[test]
    fn unknown_keys_are_named_with_their_line() {
        let text = MINIMAL.replace("effort = \"medium\"", "effort = \"medium\"\nmodelo = \"x\"");
        let error = TeamConfig::parse(&text).unwrap_err();
        assert!(error.contains("modelo"), "{error}");
        assert!(error.contains("línea"), "{error}");
    }

    #[test]
    fn unknown_values_list_the_valid_ones() {
        for (from, to, needle) in [
            ("cli = \"codex\"", "cli = \"gemini\"", "codex, claude"),
            ("effort = \"medium\"", "effort = \"max\"", "xhigh"),
            (
                "sandbox = \"workspace-write\"",
                "sandbox = \"danger-full-access\"",
                "read-only, workspace-write",
            ),
            ("version = 1", "version = 2", "versión 1"),
            ("steps = [\"impl\"]", "steps = [\"tests\"]", "«tests»"),
        ] {
            let error = TeamConfig::parse(&MINIMAL.replace(from, to)).unwrap_err();
            assert!(error.contains(needle), "{to}: {error}");
        }
    }

    #[test]
    fn a_launched_claude_role_or_a_role_without_prompt_is_refused() {
        let error = TeamConfig::parse(&MINIMAL.replace("launch = false", "")).unwrap_err();
        assert!(error.contains("director"), "{error}");
        let error =
            TeamConfig::parse(&MINIMAL.replace("prompt_file = \".agents/prompts/impl.md\"", ""))
                .unwrap_err();
        assert!(error.contains("prompt_file"), "{error}");
    }

    #[test]
    fn trailing_slash_covers_the_whole_folder() {
        assert!(path_matches("crates/core/tests/a.rs", "crates/*/tests/"));
        assert!(path_matches(
            "crates/core/tests/sub/b.rs",
            "crates/*/tests/"
        ));
        assert!(!path_matches("crates/core/src/lib.rs", "crates/*/tests/"));
        assert!(!path_matches("tests/a.rs", "crates/*/tests/"));
        assert!(path_matches("tests/a.py", "tests/"));
        assert!(!path_matches("src/tests.py", "tests/"));
        assert!(path_matches("docs/README.md", "/docs/"));
        assert!(path_matches("a/b/c.rs", "**/*.rs"));
        assert!(!path_matches("a/b/c.rs", "*.rs"));
    }

    #[test]
    fn warnings_follow_allowed_and_forbidden_paths() {
        let mut role = TeamConfig::parse(MINIMAL).unwrap().roles["impl"].clone();
        let files = vec![
            "crates/core/tests/a.rs".to_string(),
            "crates/core/src/lib.rs".to_string(),
        ];
        assert!(path_warnings(&role, &files).is_empty());
        role.allowed_paths = vec!["crates/*/tests/".into()];
        assert_eq!(
            path_warnings(&role, &files),
            ["Fichero fuera de allowed_paths: crates/core/src/lib.rs"]
        );
        role.allowed_paths.clear();
        role.forbidden_paths = vec!["crates/*/tests/".into()];
        assert_eq!(
            path_warnings(&role, &files),
            ["Fichero dentro de forbidden_paths: crates/core/tests/a.rs"]
        );
    }
}
