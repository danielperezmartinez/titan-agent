//! What the body of a session's conversation shows (docs/Decisiones
//! visuales/Ventana principal con proyectos importados y monitor por sesión
//! directora.md): the terminal or, in a director session, the monitor of one
//! of its open tasks. Pure, so the `[m]` switch and the task picker can be
//! tested without a window.
//!
//! `tasks` is always the session's open tasks as
//! [`titan_agent_core::monitor::directors`] gives them: the newest first,
//! empty when the session directs nothing.

use titan_agent_core::monitor::TaskId;
use titan_agent_core::project::project_name;
use titan_agent_core::protocol::ProjectInfo;
use titan_agent_core::session::Session;
use titan_agent_core::team::TeamRun;
use titan_agent_core::time::ago;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SessionBody {
    #[default]
    Terminal,
    Monitor {
        task: TaskId,
    },
}

/// `[m]`: the newest task's monitor, or back to the terminal. A session that
/// directs nothing has no monitor.
pub fn toggle_monitor(body: &SessionBody, tasks: &[TaskId]) -> SessionBody {
    match (body, tasks.first()) {
        (SessionBody::Terminal, Some(newest)) => SessionBody::Monitor {
            task: newest.clone(),
        },
        _ => SessionBody::Terminal,
    }
}

/// The task picker: the monitor of `task` when the session directs it.
pub fn pick_task(body: &SessionBody, task: &TaskId, tasks: &[TaskId]) -> SessionBody {
    if tasks.contains(task) {
        SessionBody::Monitor { task: task.clone() }
    } else {
        body.clone()
    }
}

/// After a snapshot: a task that closed (or changed director) gives way to
/// the newest one left, or to the terminal.
pub fn settle(body: &SessionBody, tasks: &[TaskId]) -> SessionBody {
    match body {
        SessionBody::Monitor { task } if !tasks.contains(task) => match tasks.first() {
            Some(newest) => SessionBody::Monitor {
                task: newest.clone(),
            },
            None => SessionBody::Terminal,
        },
        _ => body.clone(),
    }
}

/// The folders the draft's project menu offers, as `(folder, name)`: the
/// imported projects that exist, by name; then the projects of the other
/// sessions, by name; then the folders picked with `[d]`, in order. No
/// folder twice (trailing slashes aside).
pub fn folder_choices(
    projects: &[ProjectInfo],
    sessions: &[Session],
    picked: &[String],
) -> Vec<(String, String)> {
    let trim = |s: &str| {
        let t = s.trim_end_matches('/');
        if t.is_empty() { "/" } else { t }.to_string()
    };
    let by_name = |list: &mut Vec<(String, String)>| {
        list.sort_by(|a, b| (a.1.to_lowercase(), &a.0).cmp(&(b.1.to_lowercase(), &b.0)));
    };
    let has =
        |list: &[(String, String)], folder: &str| list.iter().any(|(f, _)| trim(f) == trim(folder));
    let mut choices: Vec<(String, String)> = Vec::new();
    for project in projects.iter().filter(|p| !p.missing) {
        if !has(&choices, &project.root) {
            choices.push((project.root.clone(), project.name.clone()));
        }
    }
    by_name(&mut choices);
    let mut others: Vec<(String, String)> = Vec::new();
    for session in sessions {
        let imported = projects
            .iter()
            .any(|p| trim(&p.root) == trim(&session.project_root));
        if !imported
            && !has(&choices, &session.project_root)
            && !has(&others, &session.project_root)
        {
            others.push((session.project_root.clone(), session.project.clone()));
        }
    }
    by_name(&mut others);
    choices.extend(others);
    for folder in picked {
        if !has(&choices, folder) {
            choices.push((folder.clone(), project_name(folder)));
        }
    }
    choices
}

/// The sessions a project's summary lists: its live ones, in the order of
/// the tree (`project_tree` without filter), children after their parent.
pub fn summary_sessions(
    root: &str,
    sessions: &[Session],
    projects: &[ProjectInfo],
    teams: &[titan_agent_core::protocol::TeamTask],
) -> Vec<String> {
    use titan_agent_core::tree::{NavRow, project_tree};
    let trim = |s: &str| s.trim_end_matches('/').to_string();
    project_tree(sessions, projects, teams, "")
        .into_iter()
        .skip_while(|row| !matches!(row, NavRow::Project { root: r, .. } if trim(r) == trim(root)))
        .skip(1)
        .take_while(|row| matches!(row, NavRow::Session { .. }))
        .filter_map(|row| match row {
            NavRow::Session { key, .. } => Some(key),
            _ => None,
        })
        .filter(|key| sessions.iter().any(|s| &s.key == key && s.state.is_live()))
        .collect()
}

/// Where the `[m]` of a team in a project's summary leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamOpen {
    /// Its director session, with that task's monitor in place of the
    /// terminal.
    Director { session: String, task: TaskId },
    /// No director session is known: the monitor in the summary itself.
    Here { task: TaskId },
}

pub fn team_open(task: &TaskId, director: Option<&str>) -> TeamOpen {
    match director {
        Some(session) => TeamOpen::Director {
            session: session.to_string(),
            task: task.clone(),
        },
        None => TeamOpen::Here { task: task.clone() },
    }
}

/// A task's line in the director's task picker: its review round as the
/// monitor counts it (`ronda N de M`, the first round before it starts),
/// or `sin revisión` for a team without one, and the time of its last event.
pub fn task_hint(run: &TeamRun, now_ms: i64) -> String {
    let round = if run.max_review_rounds == 0 {
        "sin revisión".to_string()
    } else {
        format!(
            "ronda {} de {}",
            run.review_round.max(1),
            run.max_review_rounds
        )
    };
    format!("{round} · {}", ago(run.last_ts_ms, now_ms))
}

/// A session's name in the monitor's crumb `[#] proyecto › directora`:
/// without the `proyecto: ` prefix the launches give it (the crumb already
/// names the project), so two directors of one project read apart.
pub fn crumb_name<'a>(name: &'a str, project: &str) -> &'a str {
    name.strip_prefix(project)
        .and_then(|rest| rest.strip_prefix(':'))
        .map(str::trim_start)
        .filter(|rest| !rest.is_empty())
        .unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> TaskId {
        TaskId {
            folder: "/p".into(),
            task: name.into(),
        }
    }

    #[test]
    fn monitor_follows_the_open_tasks() {
        let tasks = [id("b"), id("a")];
        let body = toggle_monitor(&SessionBody::Terminal, &tasks);
        assert_eq!(body, SessionBody::Monitor { task: id("b") });
        let body = pick_task(&body, &id("a"), &tasks);
        assert_eq!(settle(&body, &tasks), body);
        assert_eq!(
            settle(&body, &[id("b")]),
            SessionBody::Monitor { task: id("b") }
        );
        assert_eq!(settle(&body, &[]), SessionBody::Terminal);
        assert_eq!(pick_task(&body, &id("z"), &tasks), body);
        assert_eq!(toggle_monitor(&body, &tasks), SessionBody::Terminal);
        assert_eq!(
            toggle_monitor(&SessionBody::Terminal, &[]),
            SessionBody::Terminal
        );
    }

    #[test]
    fn crumb_drops_the_project_prefix() {
        assert_eq!(
            crumb_name("titan-ssh: sftp renombrar", "titan-ssh"),
            "sftp renombrar"
        );
        assert_eq!(crumb_name("titan-ssh:", "titan-ssh"), "titan-ssh:");
        assert_eq!(crumb_name("otra cosa", "titan-ssh"), "otra cosa");
    }

    #[test]
    fn task_hint_counts_rounds_like_the_monitor() {
        let mut run = TeamRun {
            task: "t".into(),
            director: Some("director".into()),
            director_session: None,
            review_role: Some("revisor".into()),
            review_round: 0,
            max_review_rounds: 2,
            closed: false,
            members: Vec::new(),
            messages: Vec::new(),
            last_ts_ms: 0,
            feed: Vec::new(),
            spans: Vec::new(),
            last_report: None,
        };
        assert_eq!(task_hint(&run, 120_000), "ronda 1 de 2 · hace 2 min");
        run.review_round = 2;
        assert_eq!(task_hint(&run, 5_000), "ronda 2 de 2 · ahora");
        run.max_review_rounds = 0;
        assert_eq!(task_hint(&run, 5_000), "sin revisión · ahora");
    }
}
