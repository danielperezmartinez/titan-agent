//! Event logs of the team tool (ADR-0010) next to the known sessions.
//!
//! A session working in `<base>/.agents/worktrees/<task>` belongs to the
//! team whose `.agents/team.toml` is in `<base>`; any other session is
//! looked up from its folder upwards. Each log is read incrementally (only
//! the bytes appended since the last poll) and folded into task runs.

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use titan_agent_core::project::team_worktree;
use titan_agent_core::session::Session;
use titan_agent_core::team::config::TeamConfig;
use titan_agent_core::team::events::{EVENTS_FILE, TeamEvent, parse_line};
use titan_agent_core::team::run::TeamRun;

/// Folders climbed from a session's folder looking for `.agents/team.toml`.
const CLIMB: usize = 6;
/// Events kept in memory per log (the oldest are dropped).
const KEEP_EVENTS: usize = 20_000;
/// Tasks without events for this long are not reported.
const REPORT_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Default)]
pub struct TeamLogs {
    logs: HashMap<PathBuf, Log>,
}

struct Log {
    base: String,
    /// Identity of the file read (a replaced file is read from the start).
    identity: Option<(u64, u64)>,
    offset: u64,
    partial: String,
    events: Vec<TeamEvent>,
}

impl TeamLogs {
    /// Reads what is new in the logs of the teams found next to `sessions`
    /// and returns their recent tasks, newest first.
    pub fn runs(&mut self, sessions: &[Session], now_ms: i64) -> Vec<TeamRun> {
        let bases: BTreeSet<PathBuf> = sessions
            .iter()
            .filter_map(|s| team_base(Path::new(&s.cwd)))
            .collect();
        let mut runs = Vec::new();
        for base in bases {
            let path = log_path(&base);
            let log = self.logs.entry(path.clone()).or_insert_with(|| Log {
                base: base.display().to_string(),
                identity: None,
                offset: 0,
                partial: String::new(),
                events: Vec::new(),
            });
            log.read(&path);
            runs.extend(
                TeamRun::fold(&log.base, &log.events)
                    .into_iter()
                    .filter(|run| now_ms - run.updated_ms <= REPORT_MS),
            );
        }
        for run in &mut runs {
            run.join(sessions);
        }
        runs.sort_by_key(|run| std::cmp::Reverse(run.updated_ms));
        runs
    }
}

impl Log {
    fn read(&mut self, path: &Path) {
        let Ok(mut file) = File::open(path) else {
            return;
        };
        let Ok(metadata) = file.metadata() else {
            return;
        };
        let length = metadata.len();
        let identity = file_identity(&metadata);
        if length < self.offset || identity != self.identity {
            // Truncated or replaced: start again.
            self.identity = identity;
            self.offset = 0;
            self.partial.clear();
            self.events.clear();
        }
        if length == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return;
        }
        self.offset += bytes.len() as u64;
        self.partial.push_str(&String::from_utf8_lossy(&bytes));
        // Only whole lines: the last one may still be being written.
        let complete = match self.partial.rfind('\n') {
            Some(end) => {
                let rest = self.partial.split_off(end + 1);
                std::mem::replace(&mut self.partial, rest)
            }
            None => return,
        };
        self.events.extend(complete.lines().filter_map(parse_line));
        if self.events.len() > KEEP_EVENTS {
            let extra = self.events.len() - KEEP_EVENTS;
            self.events.drain(..extra);
        }
    }
}

#[cfg(unix)]
fn file_identity(metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn file_identity(_metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

/// The folder that holds the `.agents/team.toml` of a session's team.
pub fn team_base(cwd: &Path) -> Option<PathBuf> {
    if let Some((owner, _)) = team_worktree(&cwd.to_string_lossy()) {
        let owner = PathBuf::from(owner);
        return owner.join(".agents/team.toml").is_file().then_some(owner);
    }
    cwd.ancestors()
        .take(CLIMB)
        .find(|dir| dir.join(".agents/team.toml").is_file())
        .map(Path::to_path_buf)
}

/// `<base>/<state_dir>/events.jsonl`; the default state folder when the
/// configuration cannot be read.
pub fn log_path(base: &Path) -> PathBuf {
    let state_dir = std::fs::read_to_string(base.join(".agents/team.toml"))
        .ok()
        .and_then(|text| TeamConfig::parse(&text).ok())
        .map(|config| config.state_dir)
        .unwrap_or_else(|| ".agents/state".into());
    base.join(state_dir).join(EVENTS_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use titan_agent_core::session::{AgentCli, SessionKind, SessionState};
    use titan_agent_core::team::events::EventKind;

    fn session(cwd: &Path) -> Session {
        Session {
            key: "cx:t1".into(),
            agent: AgentCli::Codex,
            cli_id: "t1".into(),
            name: "x".into(),
            cwd: cwd.display().to_string(),
            project_root: String::new(),
            project: String::new(),
            kind: SessionKind::Exec,
            state: SessionState::Working,
            model: None,
            effort: None,
            role: None,
            parent: None,
            started_at_ms: None,
            state_since_ms: None,
            pid: None,
            attach: None,
            stoppable: false,
            launched_here: false,
        }
    }

    #[test]
    fn logs_are_found_from_task_worktrees_and_read_incrementally() {
        let dir = std::env::temp_dir().join(format!("titan-agentd-team-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let base = dir.join("repo");
        std::fs::create_dir_all(base.join(".agents/worktrees/t/.agents")).unwrap();
        std::fs::write(
            base.join(".agents/team.toml"),
            "version = 1\nstate_dir = \"estado\"\n[roles.r]\ncli = \"codex\"\nmodel = \"m\"\nprompt_file = \"p\"\n",
        )
        .unwrap();
        // The task worktree has its own copy of team.toml: it must not win.
        std::fs::copy(
            base.join(".agents/team.toml"),
            base.join(".agents/worktrees/t/.agents/team.toml"),
        )
        .unwrap();
        let worktree = base.join(".agents/worktrees/t");
        assert_eq!(team_base(&worktree), Some(base.clone()));
        assert_eq!(team_base(&dir), None, "nothing above the team");
        std::fs::create_dir_all(base.join("src/a")).unwrap();
        assert_eq!(team_base(&base.join("src/a")), Some(base.clone()));
        let path = log_path(&base);
        assert_eq!(path, base.join("estado/events.jsonl"));

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = std::fs::File::create(&path).unwrap();
        let now = titan_agent_core::time::now_ms();
        let line = |kind: EventKind| {
            let mut e = TeamEvent::new("t", None, kind);
            e.at_ms = now;
            e.to_line()
        };
        file.write_all(line(EventKind::Note { text: "a".into() }).as_bytes())
            .unwrap();
        // Half a line: not read until it is complete.
        let second = line(EventKind::TaskClosed);
        file.write_all(&second.as_bytes()[..10]).unwrap();
        let sessions = vec![session(&worktree)];
        let mut logs = TeamLogs::default();
        let runs = logs.runs(&sessions, now);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].events.len(), 1);
        assert!(!runs[0].closed);
        assert_eq!(runs[0].sessions.len(), 0);
        file.write_all(&second.as_bytes()[10..]).unwrap();
        let runs = logs.runs(&sessions, now);
        assert_eq!(runs[0].events.len(), 2);
        assert!(runs[0].closed);
        assert!(logs.runs(&sessions, now + REPORT_MS + 1).is_empty());

        // A file replaced by another one (rotated, rewritten) is read again.
        drop(file);
        let replacement = path.with_extension("new");
        std::fs::write(&replacement, line(EventKind::Note { text: "b".into() })).unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        let runs = logs.runs(&sessions, now);
        assert_eq!(runs[0].events.len(), 1);
        assert!(!runs[0].closed);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
