use crate::session::{AgentCli, Project, Session, SessionKey, SessionKind, SessionState};
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryError {
    NotAnArray,
    Json(String),
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAnArray => formatter.write_str("expected a JSON array"),
            Self::Json(message) => write!(formatter, "invalid JSON: {message}"),
        }
    }
}

impl std::error::Error for DiscoveryError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeAgent {
    pub id: Option<String>,
    pub pid: Option<u32>,
    pub cwd: String,
    pub kind: SessionKind,
    pub started_at_ms: Option<i64>,
    pub session_id: Option<String>,
    pub name: String,
    pub status: Option<String>,
    pub state: Option<String>,
}

fn string(value: &Value, field: &str) -> Option<String> {
    value.get(field)?.as_str().map(str::to_owned)
}

fn integer(value: &Value, field: &str) -> Option<i64> {
    value.get(field)?.as_i64()
}

pub fn parse_claude_agents(json: &str) -> Result<Vec<ClaudeAgent>, DiscoveryError> {
    let value: Value =
        serde_json::from_str(json).map_err(|error| DiscoveryError::Json(error.to_string()))?;
    let entries = value.as_array().ok_or(DiscoveryError::NotAnArray)?;
    Ok(entries
        .iter()
        .filter_map(|entry| {
            let cwd = string(entry, "cwd")?;
            let name = string(entry, "name")?;
            let kind = match string(entry, "kind")?.as_str() {
                "interactive" => SessionKind::Interactive,
                "background" => SessionKind::Background,
                _ => return None,
            };
            Some(ClaudeAgent {
                id: string(entry, "id"),
                pid: entry
                    .get("pid")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok()),
                cwd,
                kind,
                started_at_ms: integer(entry, "startedAt"),
                session_id: string(entry, "sessionId"),
                name,
                status: string(entry, "status"),
                state: string(entry, "state"),
            })
        })
        .collect())
}

pub fn claude_state(status: Option<&str>, state: Option<&str>) -> SessionState {
    match state {
        Some("blocked") => SessionState::NeedsYou,
        Some("done" | "stopped") => SessionState::Finished { failed: false },
        Some("failed" | "error") => SessionState::Finished { failed: true },
        _ => match status {
            Some("busy") => SessionState::Working,
            Some("idle") => SessionState::Waiting,
            _ if state == Some("working") => SessionState::Working,
            _ => SessionState::Waiting,
        },
    }
}

pub fn claude_session(
    agent: &ClaudeAgent,
    project: Project,
    model: Option<String>,
) -> Option<Session> {
    let id = agent
        .id
        .clone()
        .or_else(|| agent.pid.map(|pid| format!("pid-{pid}")))?;
    let attach = (agent.kind == SessionKind::Background && agent.id.is_some())
        .then(|| vec!["claude".into(), "attach".into(), id.clone()]);
    Some(Session {
        key: SessionKey::new(AgentCli::ClaudeCode, id),
        name: agent.name.clone(),
        kind: agent.kind,
        state: claude_state(agent.status.as_deref(), agent.state.as_deref()),
        cwd: agent.cwd.clone(),
        project,
        model,
        parent: None,
        started_at_ms: agent.started_at_ms,
        state_since_ms: None,
        attach,
    })
}

pub fn claude_model_from_transcript(tail: &str) -> Option<String> {
    tail.lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|value| {
            if value.get("type")?.as_str()? != "assistant" {
                return None;
            }
            let model = value.get("message")?.get("model")?.as_str()?;
            (model != "<synthetic>").then(|| short_model(model))
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexThread {
    pub id: String,
    pub name: Option<String>,
    pub preview: Option<String>,
    pub cwd: String,
    pub created_at_s: Option<i64>,
    pub state: Option<SessionState>,
    pub model: Option<String>,
    pub parent_thread_id: Option<String>,
    pub source: Option<String>,
}

pub fn parse_codex_thread(value: &Value) -> Option<CodexThread> {
    let id = string(value, "id")?;
    let cwd = string(value, "cwd")?;
    let state = value
        .get("status")
        .map(codex_state)
        .unwrap_or(Some(SessionState::Waiting));
    Some(CodexThread {
        id,
        name: string(value, "name"),
        preview: string(value, "preview"),
        cwd,
        created_at_s: integer(value, "createdAt"),
        state,
        model: string(value, "model"),
        parent_thread_id: string(value, "parentThreadId"),
        source: string(value, "source"),
    })
}

pub fn codex_state(status: &Value) -> Option<SessionState> {
    match status.get("type").and_then(Value::as_str) {
        Some("idle") => Some(SessionState::Waiting),
        Some("active") => {
            let needs_you = status
                .get("activeFlags")
                .and_then(Value::as_array)
                .is_some_and(|flags| {
                    flags.iter().any(|flag| {
                        matches!(
                            flag.as_str(),
                            Some("waitingOnApproval" | "waitingOnUserInput")
                        )
                    })
                });
            Some(if needs_you {
                SessionState::NeedsYou
            } else {
                SessionState::Working
            })
        }
        Some("systemError") => Some(SessionState::Finished { failed: true }),
        Some("notLoaded") => None,
        _ => Some(SessionState::Waiting),
    }
}

pub fn codex_session(thread: &CodexThread, project: Project) -> Option<Session> {
    let state = thread.state?;
    let name = thread
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| thread.preview.as_deref().and_then(preview_name))
        .unwrap_or_else(|| format!("codex {}", thread.id.chars().take(8).collect::<String>()));
    let kind = if matches!(thread.source.as_deref(), Some("exec" | "appServer")) {
        SessionKind::Background
    } else {
        SessionKind::Interactive
    };
    Some(Session {
        key: SessionKey::new(AgentCli::Codex, thread.id.clone()),
        name,
        kind,
        state,
        cwd: thread.cwd.clone(),
        project,
        model: thread.model.as_deref().map(short_model),
        parent: thread
            .parent_thread_id
            .as_ref()
            .map(|id| SessionKey::new(AgentCli::Codex, id)),
        started_at_ms: thread
            .created_at_s
            .and_then(|seconds| seconds.checked_mul(1000)),
        state_since_ms: None,
        attach: Some(vec!["codex".into(), "resume".into(), thread.id.clone()]),
    })
}

fn preview_name(preview: &str) -> Option<String> {
    let line = preview.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    let mut chars = line.chars();
    let cut: String = chars.by_ref().take(48).collect();
    Some(if chars.next().is_some() {
        format!("{cut}…")
    } else {
        cut
    })
}

pub fn short_model(model: &str) -> String {
    let model = model
        .strip_suffix(']')
        .and_then(|value| value.rsplit_once('[').map(|(head, _)| head))
        .unwrap_or(model);
    for prefix in ["claude-", "gpt-"] {
        if let Some(rest) = model.strip_prefix(prefix) {
            return rest
                .split('-')
                .find_map(|segment| {
                    (!segment.is_empty()
                        && segment
                            .chars()
                            .all(|character| character.is_ascii_alphabetic()))
                    .then(|| segment.to_owned())
                })
                .unwrap_or_else(|| rest.to_owned());
        }
    }
    model.to_owned()
}

pub fn project_for(cwd: &str, is_repo_root: impl Fn(&str) -> bool) -> Project {
    let cwd = trim_trailing_slashes(cwd);
    let index = ["/.claude/worktrees/", "/.agents/worktrees/", "/.worktrees/"]
        .iter()
        .filter_map(|marker| cwd.find(marker))
        .min();
    if let Some(index) = index {
        return project(trim_trailing_slashes(&cwd[..index]));
    }
    let mut candidate = cwd;
    loop {
        if is_repo_root(candidate) {
            return project(candidate);
        }
        if candidate == "/" {
            break;
        }
        candidate = candidate
            .rsplit_once('/')
            .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
            .unwrap_or("/");
    }
    project(cwd)
}

fn trim_trailing_slashes(path: &str) -> &str {
    if path.len() > 1 {
        path.trim_end_matches('/').if_empty(path)
    } else {
        path
    }
}

trait IfEmpty {
    fn if_empty<'a>(&'a self, fallback: &'a str) -> &'a str;
}
impl IfEmpty for str {
    fn if_empty<'a>(&'a self, fallback: &'a str) -> &'a str {
        if self.is_empty() { fallback } else { self }
    }
}

fn project(root: &str) -> Project {
    let root = if root.is_empty() { "/" } else { root };
    let name = root
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("/")
        .to_owned();
    Project {
        name,
        root: root.to_owned(),
    }
}

pub fn display_path(path: &str, home: Option<&str>) -> String {
    let Some(home) = home.filter(|home| !home.is_empty()) else {
        return path.to_owned();
    };
    if path == home {
        return "~".to_owned();
    }
    let prefix = home.trim_end_matches('/');
    if path
        .strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('/'))
    {
        format!("~{path_tail}", path_tail = &path[prefix.len()..])
    } else {
        path.to_owned()
    }
}
