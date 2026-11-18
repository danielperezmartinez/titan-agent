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
