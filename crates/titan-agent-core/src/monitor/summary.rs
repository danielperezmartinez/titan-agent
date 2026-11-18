//! The teams of a project for its summary (docs/Decisiones visuales/Ventana
//! principal con proyectos importados y monitor por sesión directora.md):
//! one line per open task, with its director, state and headline.

use crate::protocol::TeamTask;
use crate::session::{Session, SessionState};

use super::{TaskId, board, director_of, run_of};

/// One open task of a project, as the summary lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamLine {
    pub id: TaskId,
    /// Key of its director session, when known ([`director_of`]).
    pub director: Option<String>,
    pub state: SessionState,
    pub headline: String,
    pub round: u32,
    pub max_rounds: u32,
    pub last_ts_ms: i64,
}

/// The open tasks of the project at `root`, the newest first.
pub fn project_teams(
    root: &str,
    teams: &[TeamTask],
    sessions: &[Session],
    now_ms: i64,
) -> Vec<TeamLine> {
    let trim = |s: &str| s.trim_end_matches('/').to_string();
    let mut lines: Vec<TeamLine> = teams
        .iter()
        .filter(|task| trim(&task.project_root) == trim(root))
        .filter_map(|task| {
            let run = run_of(task).filter(|run| !run.closed)?;
            let shown = board(task, &run, sessions, now_ms);
            let any = |state: SessionState| shown.agents.iter().any(|a| a.state == state);
            let state = if any(SessionState::NeedsYou) {
                SessionState::NeedsYou
            } else if any(SessionState::Working) {
                SessionState::Working
            } else {
                SessionState::Waiting
            };
            Some(TeamLine {
                id: TaskId::of(task),
                director: director_of(task, &run, sessions).map(|s| s.key.clone()),
                state,
                headline: shown.headline(now_ms),
                round: shown.round,
                max_rounds: shown.max_rounds,
                last_ts_ms: run.last_ts_ms,
            })
        })
        .collect();
    lines.sort_by(|a, b| {
        b.last_ts_ms
            .cmp(&a.last_ts_ms)
            .then_with(|| a.id.task.cmp(&b.id.task))
    });
    lines
}
