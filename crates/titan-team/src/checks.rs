//! What titan-team runs around a turn so the director does not have to
//! (docs/Decisiones/ADR-0010, "Ampliaciones del equipo v5"): the `checks` of
//! a writing role after an `ok` turn, and the gate that `ask --gate` hands to
//! a role that cannot run it (the read-only reviewer).

use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::{Role, TeamConfig};
use crate::process::{last_lines, run_shell};

/// Lines of each check kept in its `.checks.txt` file.
const CHECK_LINES: usize = 400;
/// Lines of the gate added to the role's message.
const GATE_LINES: usize = 150;

/// One check that ran, as the history keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckRun {
    pub name: String,
    pub cmd: String,
    pub exit: i32,
    pub duration_s: f64,
}

impl CheckRun {
    pub fn passed(&self) -> bool {
        self.exit == 0
    }

    /// The warning of a failed check.
    pub fn warning(&self) -> String {
        format!("comprobación {} falló (exit {})", self.name, self.exit)
    }
}

/// Runs the role's checks in `worktree`, in order and all of them even when
/// one fails. `started` gets each command before it runs (for the
/// `activity` event). Returns the runs and the text of the `.checks.txt`
/// file.
pub fn run_checks(
    team: &TeamConfig,
    role: &Role,
    worktree: &Path,
    mut started: impl FnMut(&str),
) -> (Vec<CheckRun>, String) {
    let limit = Duration::from_secs(team.project.test_timeout_s);
    let mut runs = Vec::new();
    let mut text = String::new();
    for name in &role.checks {
        // Validation guarantees every check has its command.
        let Some(cmd) = team.project.check_cmd(name) else {
            continue;
        };
        started(cmd);
        let clock = Instant::now();
        let (exit, output) = run_shell(cmd, worktree, limit);
        let run = CheckRun {
            name: name.clone(),
            cmd: cmd.to_string(),
            exit,
            duration_s: (clock.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        };
        text.push_str(&format!(
            "## {} · exit {}\n$ {}\n{}\n\n",
            run.name,
            run.exit,
            run.cmd,
            last_lines(&output, CHECK_LINES)
        ));
        runs.push(run);
    }
    (runs, text)
}

/// One line for the printed report: `fmt ok · test exit 101`.
pub fn summary(runs: &[CheckRun]) -> String {
    runs.iter()
        .map(|run| {
            if run.passed() {
                format!("{} ok", run.name)
            } else {
                format!("{} exit {}", run.name, run.exit)
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Runs the gate command `cmd` in `worktree` and returns the block added
/// to the role's message (`# Compuerta (exit N)` and its last lines).
pub fn gate(team: &TeamConfig, cmd: &str, worktree: &Path) -> String {
    let (exit, output) = run_shell(
        cmd,
        worktree,
        Duration::from_secs(team.project.test_timeout_s),
    );
    format!(
        "# Compuerta (exit {exit})\n\n{}",
        last_lines(&output, GATE_LINES)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &str, exit: i32) -> CheckRun {
        CheckRun {
            name: name.into(),
            cmd: format!("{name} cmd"),
            exit,
            duration_s: 0.1,
        }
    }

    #[test]
    fn the_summary_and_warnings_name_each_check() {
        let runs = [run("fmt", 0), run("test", 101)];
        assert_eq!(summary(&runs), "fmt ok · test exit 101");
        assert!(runs[0].passed() && !runs[1].passed());
        assert_eq!(runs[1].warning(), "comprobación test falló (exit 101)");
    }
}
