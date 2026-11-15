//! Grouping of sessions by project, derived only from their working folder.

/// Folders that hold worktrees of a project; a session working inside one of
/// them belongs to the project that contains the folder.
const WORKTREE_MARKERS: [&str; 2] = ["/.claude/worktrees/", "/.agents/worktrees/"];

/// Returns the project root for a working folder: the folder itself, or the
/// repository that owns it when it is a Claude Code or team worktree.
pub fn project_root(cwd: &str) -> String {
    let normalized = normalize(cwd);
    let mut cut = normalized.len();
    for marker in WORKTREE_MARKERS {
        if let Some(index) = normalized.find(marker) {
            cut = cut.min(index);
        }
    }
    let root = &normalized[..cut];
    if root.is_empty() {
        "/".to_string()
    } else {
        root.to_string()
    }
}

/// Returns the display name of a project root: its last path component.
pub fn project_name(root: &str) -> String {
    let normalized = normalize(root);
    normalized
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or("/")
        .to_string()
}

/// Returns the team task folder (`<root>/.agents/worktrees/<task>`) that
/// contains `cwd`, as `(owner folder, task name)`, if any. The owner folder is
/// where the team tool runs, that is, the director's working folder.
pub fn team_worktree(cwd: &str) -> Option<(String, String)> {
    let normalized = normalize(cwd);
    let marker = "/.agents/worktrees/";
    let index = normalized.rfind(marker)?;
    let owner = normalized[..index].to_string();
    let task = normalized[index + marker.len()..]
        .split('/')
        .next()
        .filter(|task| !task.is_empty())?
        .to_string();
    Some((owner, task))
}

/// Replaces `home` at the start of `path` with `~`, for display.
pub fn tilde(path: &str, home: Option<&str>) -> String {
    match home {
        Some(home) if !home.is_empty() => {
            let home = home.trim_end_matches('/');
            if path == home {
                "~".to_string()
            } else if let Some(rest) = path.strip_prefix(home).filter(|r| r.starts_with('/')) {
                format!("~{rest}")
            } else {
                path.to_string()
            }
        }
        _ => path.to_string(),
    }
}

fn normalize(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.len() > 1 {
        trimmed.trim_end_matches('/').to_string()
    } else {
        trimmed.to_string()
    }
}
