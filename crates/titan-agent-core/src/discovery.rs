use crate::{
    protocol::{Project, Session, StopAction},
    session::{AgentCli, SessionState},
};
use serde_json::Value;
pub fn claude_state(state: Option<&str>, status: Option<&str>, has_pid: bool) -> SessionState {
    match state {
        Some("working") => SessionState::Working,
        Some("blocked") => SessionState::NeedsYou,
        Some("done") => {
            if has_pid {
                SessionState::Waiting
            } else {
                SessionState::Finished { failed: false }
            }
        }
        Some("stopped") => SessionState::Finished { failed: false },
        Some("failed" | "error") => SessionState::Finished { failed: true },
        _ => {
            if status == Some("busy") {
                SessionState::Working
            } else {
                SessionState::Waiting
            }
        }
    }
}
pub fn sessions_from_claude_agents(
    json: &str,
    project_of: &dyn Fn(&str) -> Project,
) -> Result<Vec<Session>, serde_json::Error> {
    let v: Value = serde_json::from_str(json)?;
    let a = v
        .as_array()
        .ok_or_else(|| serde::de::Error::custom("expected array"))?;
    Ok(a.iter()
        .filter_map(|x| {
            let cwd = x.get("cwd")?.as_str()?;
            let id = x.get("id").and_then(Value::as_str);
            let sid = x.get("sessionId").and_then(Value::as_str);
            let raw = id.or(sid)?;
            let pid = x
                .get("pid")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok());
            let attach = id.map(|s| vec!["claude".into(), "attach".into(), s.into()]);
            Some(Session {
                id: format!("claude:{raw}"),
                cli: AgentCli::ClaudeCode,
                name: x
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| raw.chars().take(8).collect()),
                cwd: cwd.into(),
                project: project_of(cwd),
                state: claude_state(
                    x.get("state").and_then(Value::as_str),
                    x.get("status").and_then(Value::as_str),
                    pid.is_some(),
                ),
                parent: None,
                pid,
                started_at: x.get("startedAt").and_then(Value::as_i64),
                model: None,
                attach,
                stop: if id.is_some() && pid.is_some() {
                    Some(StopAction::Stop)
                } else {
                    None
                },
            })
        })
        .collect())
}
pub fn codex_state(s: &Value) -> SessionState {
    match s.get("type").and_then(Value::as_str) {
        Some("active") => {
            if s.get("activeFlags")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty())
            {
                SessionState::NeedsYou
            } else {
                SessionState::Working
            }
        }
        Some("idle") => SessionState::Waiting,
        Some("systemError") => SessionState::Finished { failed: true },
        _ => SessionState::Finished { failed: false },
    }
}
pub fn session_from_codex_thread(
    t: &Value,
    project_of: &dyn Fn(&str) -> Project,
) -> Option<Session> {
    let id = t.get("id")?.as_str()?;
    let cwd = t.get("cwd")?.as_str()?;
    let state = codex_state(&t["status"]);
    let name = t
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            t.get("preview")
                .and_then(Value::as_str)
                .and_then(|s| s.lines().next())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.chars().take(60).collect())
        })
        .unwrap_or_else(|| id.chars().take(8).collect());
    let started_at = t
        .get("createdAt")
        .and_then(Value::as_f64)
        .and_then(|n| (n.trunc() as i64).checked_mul(1000));
    Some(Session {
        id: format!("codex:{id}"),
        cli: AgentCli::Codex,
        name,
        cwd: cwd.into(),
        project: project_of(cwd),
        state,
        parent: t
            .get("parentThreadId")
            .and_then(Value::as_str)
            .map(|s| format!("codex:{s}")),
        pid: None,
        started_at,
        model: t.get("model").and_then(Value::as_str).map(str::to_owned),
        attach: Some(vec!["codex".into(), "resume".into(), id.into()]),
        stop: if matches!(state, SessionState::Working | SessionState::NeedsYou) {
            Some(StopAction::Interrupt)
        } else {
            None
        },
    })
}
pub fn project_name(root: &str) -> String {
    let p = root.trim_end_matches('/');
    if p.is_empty() {
        "/".into()
    } else {
        p.rsplit('/').next().unwrap_or(p).into()
    }
}
pub fn project_from_git_common_dir(d: &str) -> Project {
    let p = d.trim_end_matches('/');
    let root = p
        .rsplit_once('/')
        .map(|x| if x.0.is_empty() { "/" } else { x.0 })
        .unwrap_or("/");
    Project {
        root: root.into(),
        name: project_name(root),
    }
}
pub fn fallback_project(cwd: &str, has_git: &dyn Fn(&str) -> bool) -> Project {
    let mut start = cwd.trim_end_matches('/').to_owned();
    if start.is_empty() {
        start = "/".into();
    }
    for seg in ["/.claude/worktrees/", "/.agents/worktrees/"] {
        if let Some(i) = start.find(seg) {
            start.truncate(i);
            break;
        }
    }
    let mut p = start.clone();
    loop {
        if has_git(&p) {
            return Project {
                root: p.clone(),
                name: project_name(&p),
            };
        }
        if p == "/" || p.is_empty() {
            break;
        }
        p = p
            .trim_end_matches('/')
            .rsplit_once('/')
            .map(|x| {
                if x.0.is_empty() {
                    "/".into()
                } else {
                    x.0.into()
                }
            })
            .unwrap_or("/".into())
    }
    Project {
        root: cwd.into(),
        name: project_name(cwd),
    }
}
