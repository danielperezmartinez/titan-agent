use crate::session::{Project, Session, SessionKey};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Branch {
    None,
    Middle,
    Last,
}

impl Branch {
    pub fn marker(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Middle => "|-",
            Self::Last => "`-",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow {
    Project {
        project: Project,
        count: usize,
        team_live: bool,
    },
    Session {
        key: SessionKey,
        depth: usize,
        branch: Branch,
    },
}

pub fn matches(session: &Session, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return true;
    }
    [
        session.name.as_str(),
        session.project.name.as_str(),
        session.model.as_deref().unwrap_or(""),
        session.key.cli.tag(),
        session.key.cli.command(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(&filter))
        || session.key.cli.to_string().to_lowercase().contains(&filter)
}

fn session_order(a: &Session, b: &Session) -> std::cmp::Ordering {
    a.started_at_ms
        .is_none()
        .cmp(&b.started_at_ms.is_none())
        .then_with(|| a.started_at_ms.cmp(&b.started_at_ms))
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.key.cmp(&b.key))
}

pub fn build_tree(sessions: &[Session], filter: &str) -> Vec<TreeRow> {
    let mut unique = Vec::<&Session>::new();
    let mut keys = HashSet::new();
    for session in sessions {
        if keys.insert(session.key.clone()) {
            unique.push(session);
        }
    }
    let by_key: HashMap<SessionKey, &Session> =
        unique.iter().map(|s| (s.key.clone(), *s)).collect();
    let mut grouped: HashMap<Project, Vec<&Session>> = HashMap::new();
    for session in &unique {
        grouped
            .entry(session.project.clone())
            .or_default()
            .push(session);
    }
    let mut projects: Vec<Project> = grouped.keys().cloned().collect();
    projects.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.root.cmp(&b.root))
    });
    let mut output = Vec::new();
    for project in projects {
        let members = grouped.remove(&project).unwrap_or_default();
        let member_keys: HashSet<SessionKey> = members.iter().map(|s| s.key.clone()).collect();
        let mut children: HashMap<SessionKey, Vec<&Session>> = HashMap::new();
        let mut roots = Vec::new();
        for session in &members {
            match &session.parent {
                Some(parent) if member_keys.contains(parent) => {
                    children.entry(parent.clone()).or_default().push(*session)
                }
                _ => roots.push(*session),
            }
        }
        roots.sort_by(|a, b| session_order(a, b));
        for siblings in children.values_mut() {
            siblings.sort_by(|a, b| session_order(a, b));
        }
        let project_match = !filter.trim().is_empty()
            && project
                .name
                .to_lowercase()
                .contains(&filter.trim().to_lowercase());
        let mut visible = HashSet::new();
        if project_match || filter.trim().is_empty() {
            visible.extend(member_keys.iter().cloned());
        } else {
            for session in &members {
                if matches(session, filter) {
                    let mut current = Some(session.key.clone());
                    while let Some(key) = current {
                        if !visible.insert(key.clone()) {
                            break;
                        }
                        current = by_key
                            .get(&key)
                            .and_then(|s| s.parent.as_ref())
                            .filter(|p| member_keys.contains(*p))
                            .cloned();
                    }
                }
            }
        }
        if visible.is_empty() {
            continue;
        }
        // Find an artificial root inside each parent cycle after normal roots.
        let mut ordered_roots = roots;
        let mut cycle_roots = Vec::new();
        let mut considered_cycles = HashSet::<SessionKey>::new();
        for session in &members {
            let mut path = Vec::<SessionKey>::new();
            let mut positions = HashMap::<SessionKey, usize>::new();
            let mut current = Some(session.key.clone());
            while let Some(key) = current {
                if let Some(&cycle_start) = positions.get(&key) {
                    let mut cycle: Vec<&Session> = path[cycle_start..]
                        .iter()
                        .filter_map(|member_key| by_key.get(member_key).copied())
                        .collect();
                    cycle.sort_by(|a, b| session_order(a, b));
                    if let Some(root) = cycle.first()
                        && considered_cycles.insert(root.key.clone())
                    {
                        cycle_roots.push(*root);
                    }
                    break;
                }
                positions.insert(key.clone(), path.len());
                path.push(key.clone());
                current = by_key
                    .get(&key)
                    .and_then(|item| item.parent.as_ref())
                    .filter(|parent| member_keys.contains(*parent))
                    .cloned();
            }
        }
        cycle_roots.sort_by(|a, b| session_order(a, b));
        ordered_roots.extend(cycle_roots);
        let live_team = members.iter().any(|s| {
            s.state.is_live() && s.parent.as_ref().is_some_and(|p| member_keys.contains(p))
        });
        let mut flattened = Vec::<(SessionKey, usize, Branch)>::new();
        let mut emitted = HashSet::new();
        fn visit<'a>(
            s: &'a Session,
            depth: usize,
            branch: Branch,
            children: &HashMap<SessionKey, Vec<&'a Session>>,
            visible: &HashSet<SessionKey>,
            emitted: &mut HashSet<SessionKey>,
            out: &mut Vec<(SessionKey, usize, Branch)>,
        ) {
            if !visible.contains(&s.key) || !emitted.insert(s.key.clone()) {
                return;
            }
            out.push((s.key.clone(), depth, branch));
            let candidates: Vec<&&Session> = children
                .get(&s.key)
                .into_iter()
                .flatten()
                .filter(|child| visible.contains(&child.key) && !emitted.contains(&child.key))
                .collect();
            let len = candidates.len();
            for (i, child) in candidates.into_iter().enumerate() {
                visit(
                    child,
                    depth + 1,
                    if i + 1 == len {
                        Branch::Last
                    } else {
                        Branch::Middle
                    },
                    children,
                    visible,
                    emitted,
                    out,
                );
            }
        }
        for root in ordered_roots {
            if !emitted.contains(&root.key) {
                visit(
                    root,
                    0,
                    Branch::None,
                    &children,
                    &visible,
                    &mut emitted,
                    &mut flattened,
                );
            }
        }
        // This is a defensive fallback for malformed parent graphs.
        for session in &members {
            if !emitted.contains(&session.key) {
                visit(
                    session,
                    0,
                    Branch::None,
                    &children,
                    &visible,
                    &mut emitted,
                    &mut flattened,
                );
            }
        }
        let count = flattened.len();
        output.push(TreeRow::Project {
            project,
            count,
            team_live: live_team,
        });
        output.extend(
            flattened
                .into_iter()
                .map(|(key, depth, branch)| TreeRow::Session { key, depth, branch }),
        );
    }
    output
}

pub fn since_label(now_ms: i64, since_ms: i64) -> String {
    let elapsed = now_ms.saturating_sub(since_ms);
    if elapsed < 60_000 {
        return "ahora".into();
    }
    if elapsed < 3_600_000 {
        return format!("hace {} min", elapsed / 60_000);
    }
    if elapsed < 86_400_000 {
        return format!("hace {} h", elapsed / 3_600_000);
    }
    format!("hace {} d", elapsed / 86_400_000)
}
