//! Rows of the session tree in the left column: grouped by project, with the
//! sessions launched by another one nested under it with ASCII branches.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::session::Session;

/// One row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow {
    Project {
        name: String,
        root: String,
        /// Sessions of the project shown under it.
        count: usize,
        /// True when a team works in it: a live session launched by another.
        team_live: bool,
    },
    Session {
        key: String,
        /// 0 for top-level sessions, 1 for their children, and so on.
        depth: usize,
        /// ASCII branch drawn before the row: empty, `|-` or `` `- ``.
        branch: &'static str,
    },
}

/// Builds the rows for `sessions`, keeping only what matches `filter`
/// (case-insensitive, by session name, agent, model or project). A session
/// is kept when it or one of its descendants matches; when the project name
/// matches, the whole project is kept.
pub fn build_rows(sessions: &[Session], filter: &str) -> Vec<TreeRow> {
    let needle = filter.trim().to_lowercase();
    let by_key: HashMap<&str, &Session> = sessions.iter().map(|s| (s.key.as_str(), s)).collect();

    let mut children: HashMap<&str, Vec<&Session>> = HashMap::new();
    let mut projects: BTreeMap<(String, String), Vec<&Session>> = BTreeMap::new();
    for session in sessions {
        match session.parent.as_deref().filter(|p| by_key.contains_key(p)) {
            Some(parent) => children.entry(parent).or_default().push(session),
            None => projects
                .entry((session.project.to_lowercase(), session.project_root.clone()))
                .or_default()
                .push(session),
        }
    }
    for list in children.values_mut() {
        sort_sessions(list);
    }

    let mut rows = Vec::new();
    for ((_, root), mut tops) in projects {
        sort_sessions(&mut tops);
        let name = tops[0].project.clone();
        let project_matches = needle.is_empty() || name.to_lowercase().contains(&needle);
        let mut body = Vec::new();
        let mut visited = HashSet::new();
        for top in &tops {
            push_tree(
                top,
                0,
                false,
                &children,
                &needle,
                project_matches,
                &mut visited,
                &mut body,
            );
        }
        if body.is_empty() {
            continue;
        }
        let team_live = body.iter().any(|row| match row {
            TreeRow::Session { key, depth, .. } => {
                *depth > 0 && by_key.get(key.as_str()).is_some_and(|s| s.state.is_live())
            }
            TreeRow::Project { .. } => false,
        });
        rows.push(TreeRow::Project {
            name,
            root,
            count: body.len(),
            team_live,
        });
        rows.extend(body);
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn push_tree<'a>(
    session: &'a Session,
    depth: usize,
    last: bool,
    children: &HashMap<&str, Vec<&'a Session>>,
    needle: &str,
    show_all: bool,
    visited: &mut HashSet<&'a str>,
    rows: &mut Vec<TreeRow>,
) {
    if !visited.insert(session.key.as_str()) {
        return;
    }
    if !show_all && !subtree_matches(session, children, needle, &mut HashSet::new()) {
        return;
    }
    rows.push(TreeRow::Session {
        key: session.key.clone(),
        depth,
        branch: match (depth, last) {
            (0, _) => "",
            (_, true) => "`-",
            (_, false) => "|-",
        },
    });
    if let Some(kids) = children.get(session.key.as_str()) {
        let shown: Vec<&&Session> = kids
            .iter()
            .filter(|kid| show_all || subtree_matches(kid, children, needle, &mut HashSet::new()))
            .collect();
        let count = shown.len();
        for (index, kid) in shown.into_iter().enumerate() {
            push_tree(
                kid,
                depth + 1,
                index + 1 == count,
                children,
                needle,
                show_all,
                visited,
                rows,
            );
        }
    }
}

fn subtree_matches<'a>(
    session: &'a Session,
    children: &HashMap<&str, Vec<&'a Session>>,
    needle: &str,
    seen: &mut HashSet<&'a str>,
) -> bool {
    if !seen.insert(session.key.as_str()) {
        return false;
    }
    matches(session, needle)
        || children.get(session.key.as_str()).is_some_and(|kids| {
            kids.iter()
                .any(|kid| subtree_matches(kid, children, needle, seen))
        })
}

/// Returns whether a session matches the filter text.
pub fn matches(session: &Session, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let agent = session.agent.to_string().to_lowercase();
    [
        session.name.to_lowercase(),
        session.project.to_lowercase(),
        session.agent.tag().to_string(),
        agent,
        session.model.clone().unwrap_or_default().to_lowercase(),
    ]
    .iter()
    .any(|field| field.contains(needle))
}

/// Live sessions first, then the most recently started.
fn sort_sessions(list: &mut [&Session]) {
    list.sort_by(|a, b| {
        b.state
            .is_live()
            .cmp(&a.state.is_live())
            .then(b.started_at_ms.cmp(&a.started_at_ms))
            .then(a.name.cmp(&b.name))
    });
}
