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
}
