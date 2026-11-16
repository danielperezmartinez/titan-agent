//! A team task as the daemon reports it: the events of one task folded
//! together and joined to the sessions that took part in it.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use super::events::{EventKind, RoleInfo, TeamEvent, Usage};
use crate::project::project_root;
use crate::session::{AgentCli, Session};

/// Events of a task kept in the snapshot (the oldest are dropped).
pub const MAX_EVENTS: usize = 240;

/// One task of the team tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamRun {
    pub task: String,
    /// Folder that holds `.agents/` (where the director runs the tool).
    pub base_dir: String,
    /// Project the task belongs to (worktrees folded, like sessions).
    pub project_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    pub roles: Vec<RoleInfo>,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_role: Option<String>,
    #[serde(default)]
    pub max_review_rounds: u32,
    /// Review rounds opened so far.
    #[serde(default)]
    pub review_round: u32,
    #[serde(default)]
    pub closed: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
    /// Turns and tokens of every role over the whole task.
    #[serde(default)]
    pub stats: BTreeMap<String, RoleStats>,
    /// Key of the director's session (`cl:<id>`), once joined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub director_session: Option<String>,
    /// Key of each role's session (`cx:<thread id>`), once joined.
    #[serde(default)]
    pub sessions: BTreeMap<String, String>,
    /// Last [`MAX_EVENTS`] events, oldest first.
    pub events: Vec<TeamEvent>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleStats {
    pub turns: u32,
    pub usage: Usage,
}

impl TeamRun {
    /// Folds the events of one log (`base_dir` is the folder that holds
    /// `.agents/`) into one run per task, in order of creation.
    pub fn fold<'a>(
        base_dir: &str,
        events: impl IntoIterator<Item = &'a TeamEvent>,
    ) -> Vec<TeamRun> {
        let mut runs: Vec<TeamRun> = Vec::new();
        for event in events {
            let index = match runs.iter().position(|r| r.task == event.task) {
                Some(index) => index,
                None => {
                    runs.push(TeamRun::empty(base_dir, &event.task, event.at_ms));
                    runs.len() - 1
                }
            };
            runs[index].apply(event);
        }
        runs
    }

    fn empty(base_dir: &str, task: &str, at_ms: i64) -> TeamRun {
        TeamRun {
            task: task.to_string(),
            base_dir: base_dir.to_string(),
            project_root: project_root(base_dir),
            team: None,
            roles: Vec::new(),
            steps: Vec::new(),
            review_role: None,
            max_review_rounds: 0,
            review_round: 0,
            closed: false,
            created_ms: at_ms,
            updated_ms: at_ms,
            stats: BTreeMap::new(),
            director_session: None,
            sessions: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    fn apply(&mut self, event: &TeamEvent) {
        self.updated_ms = self.updated_ms.max(event.at_ms);
        if let Some(director) = &event.director {
            self.director_session = Some(format!("cl:{director}"));
        }
        match &event.kind {
            EventKind::TaskCreated {
                team,
                roles,
                steps,
                review_role,
                max_review_rounds,
                ..
            } => {
                self.created_ms = event.at_ms;
                self.team = team.clone();
                self.roles = roles.clone();
                self.steps = steps.clone();
                self.review_role = review_role.clone();
                self.max_review_rounds = *max_review_rounds;
                self.closed = false;
            }
            EventKind::TurnStarted {
                role,
                thread_id,
                round,
                ..
            } => {
                self.ensure_role(role);
                if let Some(thread) = thread_id {
                    self.sessions.insert(role.clone(), format!("cx:{thread}"));
                }
                if let Some(round) = round {
                    self.review_round = self.review_round.max(*round);
                }
            }
            EventKind::TurnFinished {
                role,
                thread_id,
                usage,
                ..
            } => {
                self.ensure_role(role);
                if let Some(thread) = thread_id {
                    self.sessions.insert(role.clone(), format!("cx:{thread}"));
                }
                let stats = self.stats.entry(role.clone()).or_default();
                stats.turns += 1;
                stats.usage.add(usage);
            }
            EventKind::Review {
                round, max_rounds, ..
            } => {
                self.review_round = self.review_round.max(*round);
                self.max_review_rounds = self.max_review_rounds.max(*max_rounds);
            }
            EventKind::Action {
                role, thread_id, ..
            } => {
                self.ensure_role(role);
                if let Some(thread) = thread_id {
                    self.sessions.insert(role.clone(), format!("cx:{thread}"));
                }
            }
            EventKind::Note { .. } => {}
            EventKind::TaskClosed => self.closed = true,
        }
        self.events.push(event.clone());
        if self.events.len() > MAX_EVENTS {
            let extra = self.events.len() - MAX_EVENTS;
            self.events.drain(..extra);
        }
    }

    /// A role seen in a turn but missing from `task_created` (a log that
    /// started mid-task, or a role added later).
    fn ensure_role(&mut self, role: &str) {
        if !self.roles.iter().any(|r| r.name == role) {
            self.roles.push(RoleInfo {
                name: role.to_string(),
                label: None,
                cli: "codex".into(),
                model: role.to_string(),
                effort: None,
                sandbox: None,
                purpose: None,
                launch: true,
            });
        }
    }

    /// Whether a team is working on this task: not closed and with activity
    /// in the last `idle_ms` (or a turn still open).
    pub fn is_active(&self, now_ms: i64, idle_ms: i64) -> bool {
        if self.closed {
            return false;
        }
        now_ms - self.updated_ms <= idle_ms || self.open_turns().next().is_some()
    }

    /// Roles whose last turn started and has not finished.
    pub fn open_turns(&self) -> impl Iterator<Item = &str> {
        let mut open: Vec<&str> = Vec::new();
        for event in &self.events {
            match &event.kind {
                EventKind::TurnStarted { role, .. } => {
                    if !open.contains(&role.as_str()) {
                        open.push(role);
                    }
                }
                EventKind::TurnFinished { role, .. } => open.retain(|r| r != role),
                EventKind::TaskClosed => open.clear(),
                _ => {}
            }
        }
        open.into_iter()
    }

    /// Keeps only the session keys that exist and, when the director's own
    /// id is not among the sessions, takes the live Claude Code session that
    /// works in `base_dir` (where the tool runs).
    pub fn join(&mut self, sessions: &[Session]) {
        let keys: HashSet<&str> = sessions.iter().map(|s| s.key.as_str()).collect();
        if self
            .director_session
            .as_deref()
            .is_some_and(|key| !keys.contains(key))
        {
            self.director_session = None;
        }
        if self.director_session.is_none() {
            let base = self.base_dir.trim_end_matches('/');
            self.director_session = sessions
                .iter()
                .find(|s| {
                    s.agent == AgentCli::ClaudeCode
                        && s.state.is_live()
                        && s.cwd.trim_end_matches('/') == base
                })
                .map(|s| s.key.clone());
        }
        self.sessions.retain(|_, key| keys.contains(key.as_str()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionKind, SessionState};
    use crate::team::events::TurnStatus;

    fn event(at_ms: i64, task: &str, kind: EventKind) -> TeamEvent {
        TeamEvent {
            v: 1,
            at_ms,
            task: task.into(),
            director: Some("d1".into()),
            kind,
        }
    }

    fn created() -> EventKind {
        EventKind::TaskCreated {
            branch: "encargo/a".into(),
            base: "main".into(),
            base_dir: "/w/repo".into(),
            team: Some("demo".into()),
            roles: vec![RoleInfo {
                name: "revisor".into(),
                label: None,
                cli: "codex".into(),
                model: "gpt-6.1-sol".into(),
                effort: Some("high".into()),
                sandbox: Some("read-only".into()),
                purpose: None,
                launch: true,
            }],
            steps: vec!["revisor".into()],
            review_role: Some("revisor".into()),
            max_review_rounds: 2,
        }
    }

    fn started(role: &str, thread: Option<&str>, round: Option<u32>) -> EventKind {
        EventKind::TurnStarted {
            role: role.into(),
            turn: 1,
            resumed: false,
            thread_id: thread.map(str::to_string),
            round,
            message: "Revisa".into(),
        }
    }

    fn finished(role: &str, thread: &str, output: u64) -> EventKind {
        EventKind::TurnFinished {
            role: role.into(),
            turn: 1,
            status: TurnStatus::Ok,
            thread_id: Some(thread.into()),
            duration_ms: 1000,
            usage: Usage {
                input_tokens: 100,
                cached_input_tokens: 50,
                output_tokens: output,
            },
            commit: None,
            changed_files: 0,
            warnings: 0,
            summary: "ok".into(),
        }
    }

    #[test]
    fn events_fold_into_one_run_per_task() {
        let events = vec![
            event(1, "a", created()),
            event(2, "a", started("revisor", None, Some(1))),
            event(
                3,
                "b",
                EventKind::Note {
                    text: "otra".into(),
                },
            ),
            event(4, "a", finished("revisor", "t1", 7)),
        ];
        let runs = TeamRun::fold("/w/repo/.claude/worktrees/x", &events);
        assert_eq!(runs.len(), 2);
        let a = &runs[0];
        assert_eq!(a.project_root, "/w/repo");
        assert_eq!(a.review_round, 1);
        assert_eq!(a.max_review_rounds, 2);
        assert_eq!(a.stats["revisor"].turns, 1);
        assert_eq!(a.stats["revisor"].usage.total(), 107);
        assert_eq!(a.sessions["revisor"], "cx:t1");
        assert_eq!(a.director_session.as_deref(), Some("cl:d1"));
        assert_eq!(a.updated_ms, 4);
        assert!(a.open_turns().next().is_none());
        assert_eq!(runs[1].task, "b");
    }

    #[test]
    fn open_turns_and_activity() {
        let events = vec![
            event(1_000, "a", created()),
            event(2_000, "a", started("revisor", None, None)),
        ];
        let mut run = TeamRun::fold("/w/repo", &events).remove(0);
        assert_eq!(run.open_turns().collect::<Vec<_>>(), ["revisor"]);
        assert!(
            run.is_active(10_000_000, 60_000),
            "an open turn keeps it live"
        );
        run.events
            .push(event(3_000, "a", finished("revisor", "t1", 1)));
        assert!(!run.is_active(10_000_000, 60_000));
        assert!(run.is_active(3_500, 60_000));
        run.closed = true;
        assert!(!run.is_active(3_500, 60_000));
    }

    #[test]
    fn unknown_roles_are_added_from_their_turns() {
        let events = vec![event(1, "a", started("mecanico", Some("t9"), None))];
        let run = TeamRun::fold("/w/repo", &events).remove(0);
        assert_eq!(run.roles[0].name, "mecanico");
        assert_eq!(run.roles[0].cli, "codex");
    }

    fn session(key: &str, agent: AgentCli, cwd: &str) -> Session {
        Session {
            key: key.into(),
            agent,
            cli_id: key.into(),
            name: key.into(),
            cwd: cwd.into(),
            project_root: cwd.into(),
            project: "repo".into(),
            kind: SessionKind::Background,
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
    fn join_keeps_known_sessions_and_falls_back_to_the_folder() {
        let events = vec![
            event(1, "a", created()),
            event(2, "a", finished("revisor", "t1", 1)),
        ];
        let mut run = TeamRun::fold("/w/repo", &events).remove(0);
        let sessions = vec![
            session("cl:other", AgentCli::ClaudeCode, "/w/repo/"),
            session("cx:t1", AgentCli::Codex, "/w/repo/.agents/worktrees/a"),
        ];
        run.join(&sessions);
        assert_eq!(run.director_session.as_deref(), Some("cl:other"));
        assert_eq!(run.sessions["revisor"], "cx:t1");

        let mut run = TeamRun::fold("/w/repo", &events).remove(0);
        run.join(&[session("cl:d1", AgentCli::ClaudeCode, "/elsewhere")]);
        assert_eq!(run.director_session.as_deref(), Some("cl:d1"));
        assert!(run.sessions.is_empty());
    }
}
