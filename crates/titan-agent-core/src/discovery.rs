//! Pure translation of what the official CLIs report into [`Session`]s.
//!
//! The daemon runs the commands and talks to `codex app-server`; this module
//! only interprets their JSON, so it can be tested with hand-written fixtures.
//! Fields that are not documented are optional: a missing field never fails
//! the whole list, it only leaves that detail unknown.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::project::{project_name, project_root, team_worktree};
use crate::session::{AgentCli, Session, SessionKind, SessionState, short_model};

/// Parses the output of `claude agents --json` (an array of sessions).
///
/// State mapping (checked with Claude Code 2.1.325):
/// `state = blocked` → te necesita; `state = done | stopped` → terminada;
/// `state = failed | error` → terminada con error; otherwise `status = busy`
/// → trabajando and `status = idle` → en espera; a lone `state = working`
/// counts as trabajando.
pub fn parse_claude_agents(json: &str) -> Result<Vec<Session>, String> {
    let value: Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let entries = value
        .as_array()
        .ok_or_else(|| "claude agents --json did not return an array".to_string())?;
    Ok(entries.iter().filter_map(claude_session).collect())
}

fn claude_session(entry: &Value) -> Option<Session> {
    let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_string);
    let session_id = text("sessionId");
    let short_id = text("id");
    let cli_id = short_id.clone().or_else(|| session_id.clone())?;
    let cwd = text("cwd").unwrap_or_default();
    let kind = match text("kind").as_deref() {
        Some("background") => SessionKind::Background,
        Some("interactive") => SessionKind::Interactive,
        _ => SessionKind::Other,
    };
    let state = claude_state(text("status").as_deref(), text("state").as_deref());
    let name = text("name")
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("claude {}", cli_id.chars().take(8).collect::<String>()));
    let pid = entry
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok());
    // Only background sessions have a short id that `claude attach` accepts.
    let background = kind == SessionKind::Background && short_id.is_some();
    let root = project_root(&cwd);
    Some(Session {
        key: format!("cl:{}", session_id.as_deref().unwrap_or(&cli_id)),
        agent: AgentCli::ClaudeCode,
        cli_id: cli_id.clone(),
        name,
        project: project_name(&root),
        project_root: root,
        cwd,
        kind,
        state,
        model: None,
        effort: None,
        role: None,
        parent: None,
        started_at_ms: entry.get("startedAt").and_then(Value::as_i64),
        state_since_ms: None,
        pid,
        attach: background.then(|| vec!["claude".into(), "attach".into(), cli_id.clone()]),
        stoppable: background && state.is_live(),
    })
}

/// Maps Claude Code's `status` and `state` to a [`SessionState`].
pub fn claude_state(status: Option<&str>, state: Option<&str>) -> SessionState {
    match state {
        Some("blocked") => return SessionState::NeedsYou,
        Some("done") | Some("stopped") | Some("completed") => {
            return SessionState::Finished { failed: false };
        }
        Some("failed") | Some("error") => return SessionState::Finished { failed: true },
        _ => {}
    }
    match status {
        Some("busy") => SessionState::Working,
        Some("idle") => SessionState::Waiting,
        _ if state == Some("working") => SessionState::Working,
        _ => SessionState::Waiting,
    }
}

/// Finds the model of the last assistant message in the tail of a Claude Code
/// transcript (`"model":"claude-opus-5-5"`) and returns its short name.
pub fn last_model_in_transcript(tail: &str) -> Option<String> {
    let needle = "\"model\":\"";
    let mut found = None;
    let mut rest = tail;
    while let Some(index) = rest.find(needle) {
        let after = &rest[index + needle.len()..];
        if let Some(end) = after.find('"') {
            let model = &after[..end];
            if model.starts_with("claude-") {
                found = Some(model);
            }
        }
        rest = after;
    }
    found.map(short_model)
}

/// Escapes a working folder the way Claude Code names its transcript folders
/// under `~/.claude/projects/` (`/` and `.` become `-`).
pub fn claude_project_dir(cwd: &str) -> String {
    cwd.chars()
        .map(|c| {
            if c == '/' || c == '.' || c == '\\' || c == ':' {
                '-'
            } else {
                c
            }
        })
        .collect()
}

/// Parses one `Thread` object of `codex app-server` (v2 protocol).
///
/// `loaded` tells whether the thread is loaded in the shared daemon
/// (`thread/loaded/list`); a thread that is not loaded is shown as finished.
/// State mapping: `active` with `waitingOnApproval` or `waitingOnUserInput` →
/// te necesita; `active` → trabajando; `idle` → en espera; `systemError` →
/// terminada con error; `notLoaded` → terminada.
pub fn codex_session(thread: &Value, loaded: bool) -> Option<Session> {
    let text = |key: &str| thread.get(key).and_then(Value::as_str).map(str::to_string);
    let id = text("id")?;
    let cwd = text("cwd").unwrap_or_default();
    let state = codex_state(thread.get("status"), loaded);
    let source = thread.get("source");
    let kind = match source.and_then(Value::as_str) {
        Some("cli") => SessionKind::Interactive,
        Some("exec") => SessionKind::Exec,
        Some("vscode") | Some("appServer") => SessionKind::Client,
        Some(_) => SessionKind::Other,
        None if source.and_then(|s| s.get("subAgent")).is_some() => SessionKind::SubAgent,
        None => SessionKind::Other,
    };
    let name = text("name")
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            text("preview")
                .map(|preview| first_words(&preview, 6))
                .filter(|preview| !preview.is_empty())
        })
        .unwrap_or_else(|| format!("codex {}", id.chars().take(8).collect::<String>()));
    let team_role = text("preview").and_then(|preview| team_role(&preview));
    let (name, role) = match (team_worktree(&cwd), team_role) {
        (Some((_, task)), Some(role)) => {
            let short = if role.contains("test") {
                "tests".to_string()
            } else {
                role.clone()
            };
            (format!("{short} · {task}"), Some(role))
        }
        _ => (name, text("agentRole").or_else(|| text("agentNickname"))),
    };
    let root = project_root(&cwd);
    let created = thread.get("createdAt").and_then(Value::as_i64);
    let updated = thread.get("updatedAt").and_then(Value::as_i64);
    let parent = text("parentThreadId").map(|parent| format!("cx:{parent}"));
    Some(Session {
        key: format!("cx:{id}"),
        agent: AgentCli::Codex,
        cli_id: id.clone(),
        name,
        project: project_name(&root),
        project_root: root,
        cwd,
        kind,
        state,
        model: text("model").map(|model| short_model(&model)),
        effort: text("reasoningEffort"),
        role,
        parent,
        started_at_ms: created.map(|seconds| seconds * 1000),
        // The last update is the best hint of when the current state began.
        state_since_ms: updated.map(|seconds| seconds * 1000),
        pid: None,
        attach: Some(vec![
            "codex".into(),
            "resume".into(),
            "--remote".into(),
            "unix://".into(),
            id,
        ]),
        stoppable: state == SessionState::Working || state == SessionState::NeedsYou,
    })
}

/// Maps a Codex `ThreadStatus` to a [`SessionState`].
pub fn codex_state(status: Option<&Value>, loaded: bool) -> SessionState {
    let kind = status.and_then(|s| s.get("type")).and_then(Value::as_str);
    match kind {
        Some("active") => {
            let waiting = status
                .and_then(|s| s.get("activeFlags"))
                .and_then(Value::as_array)
                .is_some_and(|flags| !flags.is_empty());
            if waiting {
                SessionState::NeedsYou
            } else {
                SessionState::Working
            }
        }
        Some("idle") => SessionState::Waiting,
        Some("systemError") => SessionState::Finished { failed: true },
        Some("notLoaded") => SessionState::Finished { failed: false },
        _ if loaded => SessionState::Waiting,
        _ => SessionState::Finished { failed: false },
    }
}

/// Reads the role from the first line of a team tool prompt
/// (`.agents/prompts/*.md`: "Eres el IMPLEMENTADOR de un equipo…"): the
/// upper-case words after "Eres el" / "Eres la", in lower case.
pub fn team_role(prompt: &str) -> Option<String> {
    let rest = prompt
        .trim_start()
        .strip_prefix("Eres el ")
        .or_else(|| prompt.trim_start().strip_prefix("Eres la "))?;
    let words: Vec<&str> = rest
        .split_whitespace()
        .take_while(|word| {
            let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
            !letters.is_empty() && letters.iter().all(|c| c.is_uppercase())
                || matches!(*word, "DE" | "DEL")
        })
        .collect();
    let role = words
        .join(" ")
        .trim_end_matches([',', '.', ':'])
        .to_lowercase();
    (!role.is_empty()).then_some(role)
}

fn first_words(text: &str, count: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().take(count + 1).collect();
    let mut short = words
        .iter()
        .take(count)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    if words.len() > count {
        short.push('…');
    }
    short
}

/// Puts every source together: deduplicates keys, links the sessions launched
/// by another one and fills in the roles that follow from that link.
///
/// Links, from most to least reliable:
/// 1. Codex `parentThreadId` (sub-agents).
/// 2. A Codex thread working in `<folder>/.agents/worktrees/<task>` hangs from
///    the live Claude Code session whose working folder is `<folder>`: that is
///    where the team tool (`bin/team`) creates its task worktrees.
///
/// Claude Code does not report who launched a session, so Claude sessions are
/// never nested.
pub fn link_sessions(mut sessions: Vec<Session>) -> Vec<Session> {
    let mut seen = HashSet::new();
    sessions.retain(|session| seen.insert(session.key.clone()));

    let keys: HashSet<String> = sessions.iter().map(|s| s.key.clone()).collect();
    let mut directors: HashMap<String, String> = HashMap::new();
    for session in &sessions {
        if session.agent == AgentCli::ClaudeCode && session.state.is_live() {
            directors
                .entry(session.cwd.trim_end_matches('/').to_string())
                .or_insert_with(|| session.key.clone());
        }
    }

    for session in &mut sessions {
        if session
            .parent
            .as_ref()
            .is_some_and(|parent| !keys.contains(parent))
        {
            session.parent = None;
        }
        if session.parent.is_none()
            && session.agent == AgentCli::Codex
            && let Some((owner, task)) = team_worktree(&session.cwd)
            && let Some(director) = directors.get(&owner)
        {
            session.parent = Some(director.clone());
            if session.role.is_none() {
                session.role = Some(format!("equipo · {task}"));
            }
        }
    }

    let parents: HashSet<String> = sessions.iter().filter_map(|s| s.parent.clone()).collect();
    for session in &mut sessions {
        if session.role.is_none() {
            session.role = Some(if parents.contains(&session.key) {
                "director".to_string()
            } else if session.parent.is_some() {
                "subagente".to_string()
            } else {
                "sesión suelta".to_string()
            });
        }
    }
    sessions
}
