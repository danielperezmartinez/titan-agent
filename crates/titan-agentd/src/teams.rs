//! Reader of the team events written by `titan-team`
//! (docs/Decisiones/ADR-0010): finds `.agents/team.toml` in the folders of
//! the known sessions and follows each `<state_dir>/<task>/events.jsonl`.
//!
//! Read only: it never writes or removes anything under `.agents/`. Files
//! are read incrementally from the last offset, so a long task costs only
//! its new lines on each poll.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;
use titan_agent_core::project::project_root;
use titan_agent_core::protocol::{TEAM_EVENTS_SENT, TeamTask};

/// A task whose last event is older than this is no longer shown.
pub const ACTIVE_WINDOW_MS: i64 = 12 * 60 * 60 * 1000;
/// On first sight, only this tail of a big events file is read.
const FIRST_READ_BYTES: u64 = 4 * 1024 * 1024;
/// Default `state_dir` of `team.toml`.
const DEFAULT_STATE_DIR: &str = ".agents/state";

#[derive(Default)]
struct FileState {
    offset: u64,
    /// Bytes after the last newline (a line still being written).
    partial: Vec<u8>,
    events: VecDeque<Value>,
    closed: bool,
    last_ts_ms: i64,
}

/// Follows the events files of every team folder it is shown.
#[derive(Default)]
pub struct TeamScanner {
    files: HashMap<PathBuf, FileState>,
}

impl TeamScanner {
    /// Reads what changed and returns the open tasks with recent activity,
    /// sorted by project and task.
    pub fn scan(&mut self, folders: &[String], now_ms: i64) -> Vec<TeamTask> {
        let mut seen = HashSet::new();
        let mut tasks = Vec::new();
        let mut folders: Vec<&String> = folders.iter().filter(|f| !f.is_empty()).collect();
        folders.sort();
        folders.dedup();
        for folder in folders {
            let base = Path::new(folder.as_str());
            let config = base.join(".agents").join("team.toml");
            if !config.is_file() {
                continue;
            }
            let state_dir = base.join(state_dir(&config));
            let Ok(entries) = std::fs::read_dir(&state_dir) else {
                continue;
            };
            let mut names: Vec<(String, PathBuf)> = entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_dir())
                .filter_map(|entry| {
                    let name = entry.file_name().to_str()?.to_string();
                    let file = entry.path().join("events.jsonl");
                    file.is_file().then_some((name, file))
                })
                .collect();
            names.sort();
            for (task, file) in names {
                let key = std::fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
                if !seen.insert(key.clone()) {
                    continue;
                }
                let state = self.files.entry(key).or_default();
                follow(&file, state);
                if state.closed || now_ms - state.last_ts_ms >= ACTIVE_WINDOW_MS {
                    continue;
                }
                tasks.push(TeamTask {
                    project_root: project_root(folder),
                    folder: folder.clone(),
                    task,
                    events: state.events.iter().cloned().collect(),
                });
            }
        }
        self.files.retain(|path, _| seen.contains(path));
        tasks.sort_by(|a, b| (&a.project_root, &a.task).cmp(&(&b.project_root, &b.task)));
        tasks
    }
}

/// `state_dir` of a `team.toml`, or the default when it cannot be read.
fn state_dir(config: &Path) -> String {
    std::fs::read_to_string(config)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| {
            table
                .get("state_dir")
                .and_then(toml::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| DEFAULT_STATE_DIR.to_string())
}

/// Reads the new bytes of an events file into its state.
fn follow(path: &Path, state: &mut FileState) {
    let Ok(mut file) = File::open(path) else {
        return;
    };
    let Ok(len) = file.metadata().map(|m| m.len()) else {
        return;
    };
    if len < state.offset {
        // Rewritten or truncated: start again.
        *state = FileState::default();
    }
    let mut skip_first_line = false;
    if state.offset == 0 && len > FIRST_READ_BYTES {
        state.offset = len - FIRST_READ_BYTES;
        skip_first_line = true;
    }
    if len == state.offset || file.seek(SeekFrom::Start(state.offset)).is_err() {
        return;
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return;
    }
    state.offset += bytes.len() as u64;
    let mut data = std::mem::take(&mut state.partial);
    data.extend_from_slice(&bytes);
    let complete = match data.iter().rposition(|b| *b == b'\n') {
        Some(end) => {
            state.partial = data[end + 1..].to_vec();
            &data[..end]
        }
        None => {
            state.partial = data;
            return;
        }
    };
    let mut lines = complete.split(|b| *b == b'\n');
    if skip_first_line {
        lines.next();
    }
    for line in lines {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if !value.is_object() {
            continue;
        }
        if value.get("type").and_then(Value::as_str) == Some("task_closed") {
            state.closed = true;
        }
        if let Some(ts) = value.get("ts_ms").and_then(Value::as_i64) {
            state.last_ts_ms = state.last_ts_ms.max(ts);
        }
        state.events.push_back(value);
        while state.events.len() > TEAM_EVENTS_SENT {
            state.events.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("titan-teams-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".agents/estado/demo")).unwrap();
        std::fs::write(
            dir.join(".agents/team.toml"),
            "version = 1\nstate_dir = \".agents/estado\"\n",
        )
        .unwrap();
        dir
    }

    fn append(dir: &Path, text: &str) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(".agents/estado/demo/events.jsonl"))
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn half_written_lines_wait_and_the_cap_keeps_the_newest() {
        let dir = folder("partial");
        let folders = vec![dir.to_string_lossy().to_string()];
        let mut scanner = TeamScanner::default();
        append(&dir, "{\"v\":1,\"ts_ms\":100,\"task\":\"demo\",\"type\":\"task_cre");
        assert!(scanner.scan(&folders, 200).is_empty());
        append(&dir, "ated\"}\nnot json\n");
        let tasks = scanner.scan(&folders, 200);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].events.len(), 1);
        for n in 0..TEAM_EVENTS_SENT + 5 {
            append(&dir, &format!("{{\"v\":1,\"ts_ms\":{},\"type\":\"activity\"}}\n", 200 + n));
        }
        let tasks = scanner.scan(&folders, 1000);
        assert_eq!(tasks[0].events.len(), TEAM_EVENTS_SENT);
        assert_eq!(tasks[0].events.last().unwrap()["ts_ms"], 200 + TEAM_EVENTS_SENT as i64 + 4);
        assert!(scanner.scan(&folders, 200 + ACTIVE_WINDOW_MS + 10_000).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_rewritten_file_is_read_again_and_closing_hides_the_task() {
        let dir = folder("rewrite");
        let folders = vec![dir.to_string_lossy().to_string(), String::new()];
        let mut scanner = TeamScanner::default();
        append(&dir, "{\"v\":1,\"ts_ms\":1,\"type\":\"task_created\"}\n{\"v\":1,\"ts_ms\":2,\"type\":\"decision\"}\n");
        assert_eq!(scanner.scan(&folders, 10)[0].events.len(), 2);
        std::fs::write(
            dir.join(".agents/estado/demo/events.jsonl"),
            "{\"v\":1,\"ts_ms\":5,\"type\":\"task_created\"}\n",
        )
        .unwrap();
        assert_eq!(scanner.scan(&folders, 10)[0].events.len(), 1);
        append(&dir, "{\"v\":1,\"ts_ms\":6,\"type\":\"task_closed\"}\n");
        assert!(scanner.scan(&folders, 10).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
