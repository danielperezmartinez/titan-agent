//! `.agents/team.toml` version 1: loading, validation and the typed team
//! (docs/Decisiones/ADR-0010). Validation reports every error at once, each
//! with its dotted path and a stable code, so an agent can fix the file of
//! another project without reading this code.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use toml::{Table, Value};

/// One problem of a `team.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigError {
    pub path: String,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct TeamConfig {
    /// Folder that holds `.agents/`; relative paths start here.
    pub base: PathBuf,
    pub name: String,
    pub state_dir: PathBuf,
    pub project: Project,
    pub codex_config: Vec<String>,
    pub flow: Flow,
    /// Roles in file order.
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub repo: PathBuf,
    pub base: String,
    pub worktrees_dir: PathBuf,
    pub branch_prefix: String,
    pub test_cmd: Option<String>,
    pub test_timeout_s: u64,
    pub autocommit: bool,
    pub hidden_paths: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Flow {
    pub steps: Vec<String>,
    pub review_role: Option<String>,
    pub fix_role: Option<String>,
    pub max_review_rounds: u32,
}

#[derive(Debug, Clone)]
pub struct Role {
    pub name: String,
    pub cli: String,
    pub model: String,
    pub effort: Option<String>,
    pub sandbox: String,
    pub launch: bool,
    pub prompt_file: Option<PathBuf>,
    pub output_schema: Option<PathBuf>,
    pub allowed_paths: Vec<String>,
    pub forbidden_paths: Vec<String>,
    pub config: Vec<String>,
    pub timeout_s: u64,
    pub purpose: String,
}

const TOP_KEYS: [&str; 7] = [
    "version",
    "name",
    "state_dir",
    "project",
    "codex",
    "flow",
    "roles",
];
const PROJECT_KEYS: [&str; 8] = [
    "repo",
    "base",
    "worktrees_dir",
    "branch_prefix",
    "test_cmd",
    "test_timeout_s",
    "autocommit",
    "hidden_paths",
];
const CODEX_KEYS: [&str; 1] = ["config"];
const FLOW_KEYS: [&str; 4] = ["steps", "review_role", "fix_role", "max_review_rounds"];
const ROLE_KEYS: [&str; 12] = [
    "cli",
    "model",
    "effort",
    "sandbox",
    "launch",
    "prompt_file",
    "output_schema",
    "allowed_paths",
    "forbidden_paths",
    "config",
    "timeout_s",
    "purpose",
];
pub const CLIS: [&str; 2] = ["codex", "claude"];
pub const EFFORTS: [&str; 5] = ["minimal", "low", "medium", "high", "xhigh"];
pub const SANDBOXES: [&str; 2] = ["read-only", "workspace-write"];

impl TeamConfig {
    pub fn role(&self, name: &str) -> Option<&Role> {
        self.roles.iter().find(|r| r.name == name)
    }

    /// Name of the first Claude Code role: the director of the team.
    pub fn director(&self) -> Option<&str> {
        self.roles
            .iter()
            .find(|r| r.cli == "claude")
            .map(|r| r.name.as_str())
    }

    /// Resolves a path of the file against the base folder.
    pub fn path(&self, value: &Path) -> PathBuf {
        resolve(&self.base, value)
    }
}

/// Joins `value` to `base` (unless absolute) and drops `.` components, so
/// paths shown and passed to other tools stay readable.
pub fn resolve(base: &Path, value: &Path) -> PathBuf {
    let joined = if value.is_absolute() {
        value.to_path_buf()
    } else {
        base.join(value)
    };
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Finds the configuration: `explicit` (a file, or a folder holding
/// `.agents/team.toml`) or the first `.agents/team.toml` from `cwd` up.
pub fn find(explicit: Option<&str>, cwd: &Path) -> Result<PathBuf, String> {
    if let Some(explicit) = explicit {
        let mut path = resolve(cwd, Path::new(explicit));
        if path.is_dir() {
            path = path.join(".agents").join("team.toml");
        }
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!("No existe la configuración: {}", path.display()))
        };
    }
    cwd.ancestors()
        .map(|dir| dir.join(".agents").join("team.toml"))
        .find(|path| path.is_file())
        .ok_or_else(|| "No se encuentra .agents/team.toml; usa --config RUTA".to_string())
}

/// Reads and validates a `team.toml`.
pub fn load(path: &Path) -> Result<TeamConfig, Vec<ConfigError>> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        vec![ConfigError {
            path: String::new(),
            code: "parse_error",
            message: format!("No se pudo leer {}: {error}", path.display()),
        }]
    })?;
    let base = path
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    parse(&text, &base)
}

/// Validates the text of a `team.toml` whose `.agents/` lives in `base`.
pub fn parse(text: &str, base: &Path) -> Result<TeamConfig, Vec<ConfigError>> {
    let table: Table = text.parse().map_err(|error: toml::de::Error| {
        vec![ConfigError {
            path: String::new(),
            code: "parse_error",
            message: format!("TOML mal formado: {}", error.message()),
        }]
    })?;
    let mut c = Checker {
        errors: Vec::new(),
        base: base.to_path_buf(),
    };
    c.unknown_keys(&table, "", &TOP_KEYS);
    match table.get("version") {
        None => c.error("version", "missing_key", "falta la clave obligatoria"),
        Some(Value::Integer(1)) => {}
        Some(other) => c.error(
            "version",
            "bad_version",
            &format!("la versión del formato es 1, no {other}"),
        ),
    }
    let name = c.text(&table, "", "name", true).unwrap_or_default();
    if table.contains_key("name") && name.trim().is_empty() {
        c.error("name", "bad_type", "no puede estar vacío");
    }
    let state_dir = c
        .text(&table, "", "state_dir", false)
        .unwrap_or_else(|| ".agents/state".into());

    let empty = Table::new();
    let project_table = c.table(&table, "", "project").unwrap_or(&empty);
    c.unknown_keys(project_table, "project", &PROJECT_KEYS);
    let p = "project";
    let project = Project {
        repo: resolve(
            base,
            Path::new(
                &c.text(project_table, p, "repo", false)
                    .unwrap_or(".".into()),
            ),
        ),
        base: c
            .text(project_table, p, "base", false)
            .unwrap_or_else(|| "main".into()),
        worktrees_dir: PathBuf::from(
            c.text(project_table, p, "worktrees_dir", false)
                .unwrap_or_else(|| ".agents/worktrees".into()),
        ),
        branch_prefix: c
            .text(project_table, p, "branch_prefix", false)
            .unwrap_or_else(|| "encargo/".into()),
        test_cmd: c.text(project_table, p, "test_cmd", false),
        test_timeout_s: c
            .positive(project_table, p, "test_timeout_s")
            .unwrap_or(300),
        autocommit: c.boolean(project_table, p, "autocommit").unwrap_or(false),
        hidden_paths: c.list(project_table, p, "hidden_paths"),
    };

    let codex_table = c.table(&table, "", "codex").unwrap_or(&empty);
    c.unknown_keys(codex_table, "codex", &CODEX_KEYS);
    let codex_config = c.pairs(codex_table, "codex", "config");

    let flow_table = c.table(&table, "", "flow").unwrap_or(&empty);
    c.unknown_keys(flow_table, "flow", &FLOW_KEYS);
    let flow = Flow {
        steps: c.list(flow_table, "flow", "steps"),
        review_role: c.text(flow_table, "flow", "review_role", false),
        fix_role: c.text(flow_table, "flow", "fix_role", false),
        max_review_rounds: c
            .integer(flow_table, "flow", "max_review_rounds", 0)
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
            .unwrap_or(0),
    };

    let mut roles = Vec::new();
    match table.get("roles") {
        None => c.error("roles", "missing_key", "hace falta al menos un rol"),
        Some(Value::Table(map)) if map.is_empty() => {
            c.error("roles", "missing_key", "hace falta al menos un rol")
        }
        Some(Value::Table(map)) => {
            for (name, value) in map {
                let path = format!("roles.{name}");
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
                {
                    c.error(&path, "bad_type", "el nombre de un rol cumple [a-z0-9_]+");
                }
                match value {
                    Value::Table(role) => roles.push(c.role(name, role)),
                    _ => c.error(&path, "bad_type", "un rol es una tabla [roles.<nombre>]"),
                }
            }
        }
        Some(_) => c.error("roles", "bad_type", "roles es una tabla de roles"),
    }

    let known: Vec<&str> = roles.iter().map(|r| r.name.as_str()).collect();
    if flow
        .steps
        .iter()
        .any(|step| !known.contains(&step.as_str()))
    {
        let missing: Vec<&str> = flow
            .steps
            .iter()
            .filter(|step| !known.contains(&step.as_str()))
            .map(String::as_str)
            .collect();
        c.error(
            "flow.steps",
            "unknown_role",
            &format!("roles que no existen: {}", missing.join(", ")),
        );
    }
    for (key, value) in [
        ("review_role", &flow.review_role),
        ("fix_role", &flow.fix_role),
    ] {
        if let Some(role) = value
            && !known.contains(&role.as_str())
        {
            c.error(
                &format!("flow.{key}"),
                "unknown_role",
                &format!("no existe el rol {role}"),
            );
        }
    }

    if !c.errors.is_empty() {
        return Err(c.errors);
    }
    Ok(TeamConfig {
        base: base.to_path_buf(),
        name,
        state_dir: resolve(base, Path::new(&state_dir)),
        project,
        codex_config,
        flow,
        roles,
    })
}

struct Checker {
    errors: Vec<ConfigError>,
    base: PathBuf,
}

fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "texto",
        Value::Integer(_) => "entero",
        Value::Float(_) => "decimal",
        Value::Boolean(_) => "booleano",
        Value::Datetime(_) => "fecha",
        Value::Array(_) => "lista",
        Value::Table(_) => "tabla",
    }
}

impl Checker {
    fn error(&mut self, path: &str, code: &'static str, message: &str) {
        self.errors.push(ConfigError {
            path: path.to_string(),
            code,
            message: message.to_string(),
        });
    }

    fn bad_type(&mut self, path: &str, expected: &str, value: &Value) {
        self.error(
            path,
            "bad_type",
            &format!("se esperaba {expected} y hay {}", kind(value)),
        );
    }

    fn unknown_keys(&mut self, table: &Table, prefix: &str, allowed: &[&str]) {
        for key in table.keys() {
            if key.is_empty() || !allowed.contains(&key.as_str()) {
                self.error(
                    &join(prefix, key),
                    "unknown_key",
                    "clave desconocida en el formato v1",
                );
            }
        }
    }

    fn table<'a>(&mut self, table: &'a Table, prefix: &str, key: &str) -> Option<&'a Table> {
        match table.get(key)? {
            Value::Table(inner) => Some(inner),
            other => {
                self.bad_type(&join(prefix, key), "una tabla", other);
                None
            }
        }
    }

    fn text(&mut self, table: &Table, prefix: &str, key: &str, required: bool) -> Option<String> {
        match table.get(key) {
            None if required => {
                self.error(
                    &join(prefix, key),
                    "missing_key",
                    "falta la clave obligatoria",
                );
                None
            }
            None => None,
            Some(Value::String(text)) => Some(text.clone()),
            Some(other) => {
                self.bad_type(&join(prefix, key), "un texto", other);
                None
            }
        }
    }

    fn integer(&mut self, table: &Table, prefix: &str, key: &str, min: i64) -> Option<i64> {
        match table.get(key)? {
            Value::Integer(n) if *n >= min => Some(*n),
            Value::Integer(n) => {
                self.error(
                    &join(prefix, key),
                    "bad_type",
                    &format!("{n} está fuera de rango (mínimo {min})"),
                );
                None
            }
            other => {
                self.bad_type(&join(prefix, key), "un entero", other);
                None
            }
        }
    }

    fn positive(&mut self, table: &Table, prefix: &str, key: &str) -> Option<u64> {
        self.integer(table, prefix, key, 1).map(|n| n as u64)
    }

    fn boolean(&mut self, table: &Table, prefix: &str, key: &str) -> Option<bool> {
        match table.get(key)? {
            Value::Boolean(b) => Some(*b),
            other => {
                self.bad_type(&join(prefix, key), "un booleano", other);
                None
            }
        }
    }

    fn list(&mut self, table: &Table, prefix: &str, key: &str) -> Vec<String> {
        match table.get(key) {
            None => Vec::new(),
            Some(Value::Array(items)) if items.iter().all(Value::is_str) => items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            Some(other) => {
                self.bad_type(&join(prefix, key), "una lista de textos", other);
                Vec::new()
            }
        }
    }

    /// A list of `key=value` overrides for Codex.
    fn pairs(&mut self, table: &Table, prefix: &str, key: &str) -> Vec<String> {
        let list = self.list(table, prefix, key);
        let bad: Vec<&str> = list
            .iter()
            .filter(|pair| {
                !pair
                    .split_once('=')
                    .is_some_and(|(k, _)| !k.trim().is_empty())
            })
            .map(String::as_str)
            .collect();
        if !bad.is_empty() {
            self.error(
                &join(prefix, key),
                "bad_type",
                &format!("cada entrada es clave=valor; no lo son: {}", bad.join(", ")),
            );
        }
        list
    }

    fn choice(
        &mut self,
        table: &Table,
        prefix: &str,
        key: &str,
        allowed: &[&str],
    ) -> Option<String> {
        let value = self.text(table, prefix, key, false)?;
        if allowed.contains(&value.as_str()) {
            Some(value)
        } else {
            self.error(
                &join(prefix, key),
                "unknown_value",
                &format!("«{value}» no es válido; valores: {}", allowed.join(", ")),
            );
            None
        }
    }

    fn file(&mut self, table: &Table, prefix: &str, key: &str) -> Option<PathBuf> {
        let value = self.text(table, prefix, key, false)?;
        let path = resolve(&self.base, Path::new(&value));
        if !path.is_file() {
            self.error(
                &join(prefix, key),
                "missing_file",
                &format!("no existe el fichero {value}"),
            );
        }
        Some(path)
    }

    fn role(&mut self, name: &str, table: &Table) -> Role {
        let p = format!("roles.{name}");
        self.unknown_keys(table, &p, &ROLE_KEYS);
        let cli = match table.get("cli") {
            None => {
                self.error(
                    &join(&p, "cli"),
                    "missing_key",
                    "falta la clave obligatoria",
                );
                String::new()
            }
            Some(_) => self.choice(table, &p, "cli", &CLIS).unwrap_or_default(),
        };
        let model = self.text(table, &p, "model", true).unwrap_or_default();
        if table.contains_key("model") && model.trim().is_empty() {
            self.error(&join(&p, "model"), "bad_type", "no puede estar vacío");
        }
        let launch = self.boolean(table, &p, "launch").unwrap_or(true);
        if cli == "claude" && launch {
            self.error(
                &join(&p, "launch"),
                "unknown_value",
                "un rol de Claude Code lleva launch = false: titan-team no lanza claude",
            );
        }
        let prompt_file = self.file(table, &p, "prompt_file");
        if cli == "codex" && launch && !table.contains_key("prompt_file") {
            self.error(
                &join(&p, "prompt_file"),
                "missing_key",
                "un rol de Codex que se lanza necesita su prompt",
            );
        }
        Role {
            name: name.to_string(),
            model,
            effort: self.choice(table, &p, "effort", &EFFORTS),
            sandbox: self
                .choice(table, &p, "sandbox", &SANDBOXES)
                .unwrap_or_else(|| "read-only".into()),
            launch,
            prompt_file,
            output_schema: self.file(table, &p, "output_schema"),
            allowed_paths: self.list(table, &p, "allowed_paths"),
            forbidden_paths: self.list(table, &p, "forbidden_paths"),
            config: self.pairs(table, &p, "config"),
            timeout_s: self.positive(table, &p, "timeout_s").unwrap_or(900),
            purpose: self.text(table, &p, "purpose", false).unwrap_or_default(),
            cli,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(text: &str) -> Vec<(String, &'static str)> {
        parse(text, Path::new("/nonexistent"))
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.path, e.code))
            .collect()
    }

    #[test]
    fn roles_keep_file_order_and_the_first_claude_role_directs() {
        let text = "version = 1\nname = \"t\"\n[roles.zeta]\ncli = \"codex\"\nmodel = \"m\"\nlaunch = false\n\
                    [roles.alpha]\ncli = \"claude\"\nmodel = \"opus\"\nlaunch = false\n";
        let team = parse(text, Path::new("/x")).unwrap();
        let names: Vec<&str> = team.roles.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["zeta", "alpha"]);
        assert_eq!(team.director(), Some("alpha"));
        assert_eq!(team.state_dir, PathBuf::from("/x/.agents/state"));
        assert_eq!(team.role("zeta").unwrap().sandbox, "read-only");
    }

    #[test]
    fn types_values_and_references_are_reported_with_their_paths() {
        let found = errors(
            "version = 1\nname = 3\n[flow]\nsteps = [\"ghost\"]\nmax_review_rounds = -1\n\
             [roles.a]\ncli = \"gemini\"\nmodel = \"m\"\neffort = \"huge\"\ntimeout_s = \"9\"\n[roles.B]\ncli = \"codex\"\nmodel = \"\"\nlaunch = false\n",
        );
        for expected in [
            ("name", "bad_type"),
            ("flow.steps", "unknown_role"),
            ("flow.max_review_rounds", "bad_type"),
            ("roles.a.cli", "unknown_value"),
            ("roles.a.effort", "unknown_value"),
            ("roles.a.timeout_s", "bad_type"),
            ("roles.B", "bad_type"),
            ("roles.B.model", "bad_type"),
        ] {
            assert!(
                found.contains(&(expected.0.to_string(), expected.1)),
                "{expected:?} in {found:?}"
            );
        }
        assert_eq!(
            errors("name = \"x\"\n[roles]\n")[0],
            ("version".into(), "missing_key")
        );
        let found = errors(
            "version = 1\nname = \"x\"\n[codex]\nconfig = [\"a=1\", \"nope\"]\n[roles.r]\ncli = \"codex\"\nmodel = \"m\"\nlaunch = false\nconfig = [\"=x\"]\n",
        );
        assert!(
            found.contains(&("codex.config".into(), "bad_type")),
            "{found:?}"
        );
        assert!(
            found.contains(&("roles.r.config".into(), "bad_type")),
            "{found:?}"
        );
    }

    #[test]
    fn resolve_drops_dot_components() {
        assert_eq!(
            resolve(Path::new("/a/b"), Path::new("./c/../d")),
            PathBuf::from("/a/b/d")
        );
        assert_eq!(
            resolve(Path::new("/a"), Path::new("/abs/./x")),
            PathBuf::from("/abs/x")
        );
    }
}
