//! Per-task state (`<state_dir>/<task>.json`, the same shape the Python
//! prototype wrote) and the append-only events log
//! (`<state_dir>/<task>/events.jsonl`, docs/Decisiones/ADR-0010).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use titan_agent_core::team::{EventKind, TeamEvent, Usage};

use crate::Failure;
use crate::config::TeamConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub task: String,
    pub created: String,
    pub repo: String,
    pub worktree: String,
    pub branch: String,
    pub base: String,
    pub base_commit: String,
    pub brief: String,
    #[serde(default)]
    pub review_rounds: u32,
    #[serde(default)]
    pub roles: BTreeMap<String, RoleState>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleState {
    pub cli: String,
    pub model: String,
    pub thread_id: Option<String>,
    #[serde(default)]
    pub turns: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub n: u32,
    pub ts: String,
    pub role: String,
    pub turn: u32,
    pub thread_id: Option<String>,
    pub resumed: bool,
    pub status: String,
    pub duration_s: f64,
    #[serde(default)]
    pub usage: Usage,
    pub output: String,
    pub commit: Option<String>,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// A task name: `[a-z0-9-]+`, 1 to 64 chars.
pub fn check_task_name(task: &str) -> Result<(), Failure> {
    let valid = !task.is_empty()
        && task.chars().count() <= 64
        && task
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if valid {
        Ok(())
    } else {
        Err(Failure::usage(
            "El encargo debe cumplir [a-z0-9-]+ y tener de 1 a 64 caracteres",
        ))
    }
}

pub fn state_file(team: &TeamConfig, task: &str) -> PathBuf {
    team.state_dir.join(format!("{task}.json"))
}

pub fn task_dir(team: &TeamConfig, task: &str) -> PathBuf {
    team.state_dir.join(task)
}

pub fn load(team: &TeamConfig, task: &str) -> Result<TaskState, Failure> {
    check_task_name(task)?;
    let path = state_file(team, task);
    let text = std::fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Failure::usage(format!("No existe el encargo {task}"))
        } else {
            Failure::usage(format!("No se pudo leer el estado de {task}: {error}"))
        }
    })?;
    serde_json::from_str(&text)
        .map_err(|error| Failure::usage(format!("No se pudo leer el estado de {task}: {error}")))
}

/// Writes the state atomically (temporary file and rename).
pub fn save(team: &TeamConfig, state: &TaskState) -> Result<(), Failure> {
    let path = state_file(team, &state.task);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(Failure::io)?;
    }
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let text = serde_json::to_string_pretty(state).map_err(|e| Failure::usage(e.to_string()))?;
    std::fs::write(&temp, text + "\n").map_err(Failure::io)?;
    std::fs::rename(&temp, &path).map_err(Failure::io)
}

/// `path` relative to `base` when it is inside it, for the state file.
pub fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Current UTC time as `2026-11-16T17:35:00+00:00`.
pub fn now_iso() -> String {
    iso(now_ms() / 1000)
}

fn iso(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+00:00",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The events log of one task. Each event is one line written at once.
pub struct EventLog {
    path: PathBuf,
    task: String,
}

impl EventLog {
    pub fn new(team: &TeamConfig, task: &str) -> EventLog {
        EventLog {
            path: task_dir(team, task).join("events.jsonl"),
            task: task.to_string(),
        }
    }

    /// Appends an event. A failure to log never stops the team's work: it
    /// is reported on stderr.
    pub fn append(&self, kind: EventKind) {
        let event = TeamEvent::new(now_ms(), &self.task, kind);
        let mut line = match serde_json::to_string(&event) {
            Ok(line) => line,
            Err(error) => {
                eprintln!("aviso: no se pudo escribir un evento: {error}");
                return;
            }
        };
        line.push('\n');
        let written = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut file| file.write_all(line.as_bytes()));
        if let Err(error) = written {
            eprintln!(
                "aviso: no se pudo escribir en {}: {error}",
                self.path.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates_are_utc_civil_dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(iso(1_794_850_058), "2026-11-16T17:27:38+00:00");
        assert_eq!(iso(951_782_400), "2000-02-29T00:00:00+00:00");
    }

    #[test]
    fn task_names_follow_the_contract() {
        assert!(check_task_name("fase-3-tests").is_ok());
        for bad in ["", "Fase", "a_b", "a/b", &"x".repeat(65)] {
            assert_eq!(check_task_name(bad).unwrap_err().code, 2, "{bad}");
        }
        assert!(check_task_name(&"x".repeat(64)).is_ok());
    }
}
