//! Reader of the team events written by `titan-team`
//! (docs/Decisiones/ADR-0010): finds `.agents/team.toml` in the folders of
//! the known sessions and follows each `<state_dir>/<task>/events.jsonl`.
//!
//! Read only: it never writes or removes anything under `.agents/`. Files
//! are read incrementally from the last offset, so a long task costs only
//! its new lines on each poll.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;
use titan_agent_core::project::project_root;
use titan_agent_core::protocol::{RoleTotals, TEAM_EVENTS_SENT, TeamTask};
use titan_agent_core::team::{EventKind, STATUS_BUSY, parse_event};

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
    created: Option<Value>,
    totals: BTreeMap<String, RoleTotals>,
    director_session: Option<String>,
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
                if state.events.is_empty()
                    || state.closed
                    || now_ms - state.last_ts_ms >= ACTIVE_WINDOW_MS
                {
                    continue;
                }
                tasks.push(TeamTask {
                    project_root: project_root(folder),
                    folder: folder.clone(),
                    task,
                    events: state.events.iter().cloned().collect(),
                    created: state.created.clone(),
                    totals: state.totals.clone(),
                    director_session: state.director_session.clone(),
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
        // Every JSON object is forwarded, but only valid events of a known
        // version close the task, count or set its time.
        if let Some(event) = parse_event(&value) {
            state.last_ts_ms = event.ts_ms;
            if event.director_session.is_some() {
                state.director_session = event.director_session.clone();
            }
            match &event.kind {
                EventKind::TaskClosed { .. } => state.closed = true,
                EventKind::TaskCreated { .. } => state.created = Some(value.clone()),
                EventKind::TurnFinished {
                    role,
                    usage,
                    status,
                    ..
                } => {
                    let totals = state.totals.entry(role.clone()).or_default();
                    // A busy thread did not run the turn (as in TeamRun).
                    if status != STATUS_BUSY {
                        totals.turns += 1;
                    }
                    totals.tokens += usage.input_tokens + usage.output_tokens;
                }
                _ => {}
            }
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

    fn decision(ts: i64) -> String {
        format!(
            "{{\"v\":1,\"ts_ms\":{ts},\"task\":\"demo\",\"type\":\"decision\",\"by\":null,\"text\":\"x\"}}\n"
        )
    }

    fn closed(ts: i64, v: u32) -> String {
        format!(
            "{{\"v\":{v},\"ts_ms\":{ts},\"task\":\"demo\",\"type\":\"task_closed\",\"delete_branch\":false}}\n"
        )
    }

    #[test]
    fn half_written_lines_wait_and_the_cap_keeps_the_newest() {
        let dir = folder("partial");
        let folders = vec![dir.to_string_lossy().to_string()];
        let mut scanner = TeamScanner::default();
        let first = decision(100);
        append(&dir, &first[..20]);
        assert!(scanner.scan(&folders, 200).is_empty());
        append(&dir, &format!("{}not json\n", &first[20..]));
        let tasks = scanner.scan(&folders, 200);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].events.len(), 1);
        for n in 0..TEAM_EVENTS_SENT + 5 {
            append(&dir, &decision(200 + n as i64));
        }
        let tasks = scanner.scan(&folders, 1000);
        assert_eq!(tasks[0].events.len(), TEAM_EVENTS_SENT);
        assert_eq!(
            tasks[0].events.last().unwrap()["ts_ms"],
            200 + TEAM_EVENTS_SENT as i64 + 4
        );
        assert!(
            scanner
                .scan(&folders, 200 + ACTIVE_WINDOW_MS + 10_000)
                .is_empty()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_a_valid_close_hides_the_task_and_the_last_event_sets_its_time() {
        let dir = folder("close");
        let folders = vec![dir.to_string_lossy().to_string(), String::new()];
        let mut scanner = TeamScanner::default();
        append(&dir, &decision(ACTIVE_WINDOW_MS * 3));
        append(&dir, &decision(5));
        // The newest line is older: the task has expired at 13 h.
        assert!(scanner.scan(&folders, 13 * 3_600_000).is_empty());
        append(&dir, &decision(13 * 3_600_000));
        append(&dir, "{\"type\":\"task_closed\"}\n");
        append(&dir, &closed(13 * 3_600_000, 99));
        let tasks = scanner.scan(&folders, 13 * 3_600_000);
        assert_eq!(tasks.len(), 1, "malformed or future closes are ignored");
        assert_eq!(tasks[0].events.len(), 5, "but still forwarded");
        append(&dir, &closed(13 * 3_600_000, 1));
        assert!(scanner.scan(&folders, 13 * 3_600_000).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn creation_and_totals_survive_the_cap_and_rewrites_start_over() {
        let dir = folder("totals");
        let folders = vec![dir.to_string_lossy().to_string()];
        let mut scanner = TeamScanner::default();
        append(
            &dir,
            "{\"v\":1,\"ts_ms\":1,\"task\":\"demo\",\"type\":\"task_created\",\"base\":\"main\",\"base_commit\":\"c\",\"branch\":\"b\",\"worktree\":\"w\",\"team\":{\"name\":\"t\"}}\n",
        );
        let finished = "{\"v\":1,\"ts_ms\":2,\"task\":\"demo\",\"director_session\":\"s-1\",\"type\":\"turn_finished\",\"role\":\"rev\",\"turn\":1,\"status\":\"ok\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}\n";
        append(&dir, finished);
        append(&dir, finished);
        for n in 0..TEAM_EVENTS_SENT {
            append(&dir, &decision(3 + n as i64));
        }
        let task = &scanner.scan(&folders, 1000)[0];
        assert_ne!(task.events[0]["type"], "task_created");
        assert_eq!(task.created.as_ref().unwrap()["type"], "task_created");
        assert_eq!(task.director_session.as_deref(), Some("s-1"));
        assert_eq!(
            task.totals["rev"],
            RoleTotals {
                turns: 2,
                tokens: 30
            }
        );
        std::fs::write(dir.join(".agents/estado/demo/events.jsonl"), decision(9)).unwrap();
        let task = &scanner.scan(&folders, 1000)[0];
        assert_eq!(task.events.len(), 1);
        assert!(task.created.is_none() && task.totals.is_empty());
        assert!(task.director_session.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
