//! State of each task in `<state_dir>/<task>.json`, in the same format as
//! the Python prototype so tasks started with it can go on.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use titan_agent_core::team::events::Usage;

use crate::context::Context;
use crate::{Result, TeamError};

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
    pub history: Vec<Turn>,
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
pub struct Turn {
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

/// Task names: lower case letters, digits and `-` (they name a branch and a
/// folder).
pub fn check_task_name(task: &str) -> Result<()> {
    let ok = !task.is_empty()
        && task
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(TeamError::new("El encargo debe cumplir [a-z0-9-]+"))
    }
}

pub fn state_file(ctx: &Context, task: &str) -> PathBuf {
    ctx.state_dir.join(format!("{task}.json"))
}

pub fn task_dir(ctx: &Context, task: &str) -> PathBuf {
    ctx.state_dir.join(task)
}

pub fn load(ctx: &Context, task: &str) -> Result<TaskState> {
    check_task_name(task)?;
    let path = state_file(ctx, task);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            TeamError::new(format!("No existe el encargo {task}"))
        } else {
            TeamError::new(format!("No se pudo leer el estado de {task}: {e}"))
        }
    })?;
    serde_json::from_str(&text)
        .map_err(|e| TeamError::new(format!("No se pudo leer el estado de {task}: {e}")))
}

/// Writes the state atomically (temporary file and rename).
pub fn save(ctx: &Context, state: &TaskState) -> Result<()> {
    let path = state_file(ctx, &state.task);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| TeamError::new(format!("No se pudo crear {}: {e}", dir.display())))?;
    }
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut text = serde_json::to_string_pretty(state).expect("the task state serializes");
    text.push('\n');
    std::fs::write(&temp, text)
        .and_then(|_| std::fs::rename(&temp, &path))
        .map_err(|e| TeamError::new(format!("No se pudo guardar {}: {e}", path.display())))
}

/// File of turn `n` of `role` in the task folder: `01-revisor.json`.
pub fn output_name(n: u32, role: &str, suffix: &str) -> String {
    let safe: String = role
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{n:02}-{safe}.{suffix}")
}
