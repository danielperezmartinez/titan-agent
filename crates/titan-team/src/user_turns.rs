//! The turns the user had with a role between two turns of titan-team
//! (from the chat or Codex's terminal, ADR-0013): finding them in
//! `thread/turns/list` and committing what they left.

use std::path::Path;

use serde_json::Value;
use titan_agent_core::team::{TEXT_LIMIT, clip};

use titan_agent_core::team::EventKind;

use crate::Failure;
use crate::config::{Role, TeamConfig};
use crate::process::{git, git_ok};
use crate::state::{EventLog, TaskState};

/// The user's turns: the turns of `thread/turns/list` (newest first) that
/// come before the newest one whose id is in `own` (the role's turns of
/// titan-team). None when no turn is the role's: the history cannot tell.
pub fn user_turns<'a>(turns: &'a [Value], own: &[String]) -> Vec<&'a Value> {
    let id = |turn: &Value| turn.get("id").and_then(Value::as_str).map(str::to_string);
    let Some(last_own) = turns
        .iter()
        .position(|turn| id(turn).is_some_and(|id| own.contains(&id)))
    else {
        return Vec::new();
    };
    turns[..last_own].iter().collect()
}

/// The text of the first `userMessage` of the oldest of `turns` (newest
/// first), cut to [`TEXT_LIMIT`] characters.
pub fn first_text(turns: &[&Value]) -> String {
    let Some(oldest) = turns.last() else {
        return String::new();
    };
    let text = oldest
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|item| item.get("type").and_then(Value::as_str) == Some("userMessage"))
        .and_then(|item| item.get("content").and_then(Value::as_array))
        .into_iter()
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    clip(&text, TEXT_LIMIT)
}

/// Commits everything uncommitted in `wt` with `message` (with a fallback
/// identity when git has none). Returns the short hash and the files, or
/// nothing when there was nothing to commit.
pub fn commit_all(wt: &Path, message: &str) -> Result<(Option<String>, Vec<String>), Failure> {
    git(wt, &["add", "-A"])?;
    if git_ok(wt, &["diff", "--cached", "--quiet"]) {
        return Ok((None, Vec::new()));
    }
    let identity = git_ok(wt, &["config", "user.name"]) && git_ok(wt, &["config", "user.email"]);
    let mut args = Vec::new();
    if !identity {
        args.extend(["-c", "user.name=team", "-c", "user.email=team@localhost"]);
    }
    args.extend(["commit", "-m", message]);
    git(wt, &args)?;
    let hash = git(wt, &["rev-parse", "--short", "HEAD"])?
        .trim()
        .to_string();
    let changed = git(wt, &["diff", "--name-only", "HEAD~1..HEAD"])?
        .lines()
        .map(str::to_string)
        .collect();
    Ok((Some(hash), changed))
}

/// The user's turns found before a continued turn, as `ask` reports them.
pub struct UserTurns {
    pub turns: u32,
    pub text: String,
    pub commit: Option<String>,
}

/// Finds the user's turns with the role since its last turn of
/// titan-team; if there are any, commits what they left (a writing role
/// with `autocommit`) and writes `user_turn`. Problems only warn: the
/// director's turn still runs.
pub fn record(
    team: &TeamConfig,
    role: &Role,
    state: &TaskState,
    wt: &Path,
    thread: &str,
    turns: &[Value],
    log: &EventLog,
) -> Option<UserTurns> {
    let own: Vec<String> = state
        .history
        .iter()
        .filter(|entry| entry.role == role.name)
        .filter_map(|entry| entry.turn_id.clone())
        .collect();
    let found = user_turns(turns, &own);
    if found.is_empty() {
        return None;
    }
    let text = first_text(&found);
    let (commit, changed_files) = if team.project.autocommit && role.sandbox == "workspace-write" {
        let message = format!("team: turnos del usuario con {}", role.name);
        commit_all(wt, &message).unwrap_or_else(|error| {
            eprintln!(
                "aviso: no se pudo hacer el commit de los turnos del usuario: {}",
                error.message
            );
            (None, Vec::new())
        })
    } else {
        (None, Vec::new())
    };
    if let Err(error) = log.append(EventKind::UserTurn {
        role: role.name.clone(),
        thread_id: Some(thread.to_string()),
        turns: found.len() as u32,
        text: text.clone(),
        commit: commit.clone(),
        changed_files,
    }) {
        eprintln!("aviso: {}", error.message);
    }
    Some(UserTurns {
        turns: found.len() as u32,
        text,
        commit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn turn(id: &str, text: &str) -> Value {
        json!({"id": id, "items": [{"type": "userMessage", "content": [{"type": "text", "text": text}]}]})
    }

    #[test]
    fn user_turns_are_the_ones_after_the_last_own() {
        let turns = [
            turn("u2", "otra"),
            turn("u1", "hola"),
            turn("t2", "x"),
            turn("t1", "y"),
        ];
        let own = ["t1".to_string(), "t2".to_string()];
        let found = user_turns(&turns, &own);
        assert_eq!(found.len(), 2);
        assert_eq!(first_text(&found), "hola");
        assert!(user_turns(&turns[2..], &own).is_empty());
        assert!(
            user_turns(&turns, &[]).is_empty(),
            "no own turn: cannot tell"
        );
        let long = [turn("u", &"x".repeat(300)), turn("t1", "")];
        assert_eq!(
            first_text(&user_turns(&long, &own)).chars().count(),
            TEXT_LIMIT
        );
    }
}
