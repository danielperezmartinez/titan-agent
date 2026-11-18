//! What the project monitor draws (docs/Decisiones visuales/Monitor del
//! proyecto en tres vistas.md), joined from a team task's events, the
//! sessions the daemon knows and the time. Pure, so the three views only
//! draw a [`Board`].

use std::collections::BTreeMap;

use crate::project::project_root;
use crate::protocol::{TeamTask, TerminalRef};
use crate::session::{AgentCli, Session, SessionState, short_model};
use crate::team::{EventKind, FeedItem, TeamRun, USER, parse_event};

/// A message counts as travelling (dot on the edge, «hablando») this long.
pub const MESSAGE_LIVE_MS: i64 = 6_000;

/// One node, lane or card of the monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    /// Role name, or [`USER`] for the user.
    pub id: String,
    /// Short name shown in bold: `opus`, `sol`, `tú`.
    pub title: String,
    pub cli: Option<AgentCli>,
    /// Role as shown: `director`, `disenador tests`, `usuario`.
    pub role: String,
    pub model: String,
    pub effort: Option<String>,
    pub state: SessionState,
    pub action: String,
    pub since_ms: Option<i64>,
    /// Session to open on click.
    pub session: Option<String>,
    pub turns: u32,
    pub tokens: u64,
    pub is_director: bool,
    /// Start of the director's session, for «en esta sesión N min».
    pub started_at_ms: Option<i64>,
    /// Codex thread of the role, even when the daemon does not list its
    /// session: a click opens it read-only.
    pub thread: Option<String>,
    /// Its thread is open as a writer in the app (a terminal of the daemon
    /// for `cx:<thread>`); see [`Board::mark_writers`].
    pub writer_open: bool,
    /// Its last turn was refused because the thread was open elsewhere.
    pub busy: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub at_ms: i64,
    pub from: String,
    pub to: String,
    pub text: String,
    /// CLI of the sender (its colour); `None` for the user.
    pub sender: Option<AgentCli>,
}

/// A time span of an agent: working, or waiting for the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub agent: String,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub needs_you: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub task: String,
    pub review_role: Option<String>,
    pub round: u32,
    pub max_rounds: u32,
    /// The user first, then the director, then the other roles.
    pub agents: Vec<Agent>,
    pub messages: Vec<Message>,
    pub feed: Vec<FeedItem>,
    pub spans: Vec<Span>,
    pub closed: bool,
}

/// One bar of a card's activity histogram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    Idle,
    Working,
    /// A message sent or received; the colour is the sender's.
    Message(Option<AgentCli>),
    NeedsYou,
}

/// The most recent open task of a project, with its summary.
pub fn latest_team(teams: &[TeamTask], root: &str) -> Option<(TeamTask, TeamRun)> {
    teams
        .iter()
        .filter(|task| task.project_root == root)
        .filter_map(|task| run_of(task).map(|run| (task.clone(), run)))
        .filter(|(_, run)| !run.closed)
        .max_by_key(|(_, run)| run.last_ts_ms)
}

/// Summary of a task as the daemon sends it: its last events, with the
/// `task_created` event put back in front when it fell out of them, and the
/// turns, tokens and director session of the whole file.
pub fn run_of(task: &TeamTask) -> Option<TeamRun> {
    let mut events: Vec<_> = task.events.iter().filter_map(parse_event).collect();
    let has_created = events
        .iter()
        .any(|e| matches!(e.kind, EventKind::TaskCreated { .. }));
    if !has_created && let Some(created) = task.created.as_ref().and_then(parse_event) {
        events.insert(0, created);
    }
    let mut run = TeamRun::from_events(&events)?;
    if task.director_session.is_some() {
        run.director_session = task.director_session.clone();
    }
    for member in &mut run.members {
        if let Some(totals) = task.totals.get(&member.role) {
            member.turns = totals.turns.max(member.turns);
            member.tokens = totals.tokens.max(member.tokens);
        }
    }
    Some(run)
}

fn same_folder(a: &str, b: &str) -> bool {
    let trim = |s: &str| s.trim_end_matches('/').to_string();
    trim(a) == trim(b)
}

fn role_label(role: &str) -> String {
    role.replace('_', " ")
}

/// One open team task, by its team folder and name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId {
    pub folder: String,
    pub task: String,
}

impl TaskId {
    pub fn of(task: &TeamTask) -> TaskId {
        TaskId {
            folder: task.folder.clone(),
            task: task.task.clone(),
        }
    }

    pub fn matches(&self, task: &TeamTask) -> bool {
        self.folder == task.folder && self.task == task.task
    }
}

/// The session that directs a task (ADR-0010). The events name the
/// director's Claude Code session when titan-team ran inside one: that exact
/// session is the director. Without it (or if the daemon does not list it),
/// several sessions can share the team folder and the busiest live one is
/// taken (it is the one running the tool). `None` for a team without a
/// director role.
pub fn director_of<'a>(
    task: &TeamTask,
    run: &TeamRun,
    sessions: &'a [Session],
) -> Option<&'a Session> {
    run.director.as_ref()?;
    let exact = run
        .director_session
        .as_ref()
        .map(|id| format!("cl:{id}"))
        .and_then(|key| sessions.iter().find(|s| s.key == key));
    exact.or_else(|| {
        sessions
            .iter()
            .filter(|s| s.agent == AgentCli::ClaudeCode && same_folder(&s.cwd, &task.folder))
            .min_by_key(|s| {
                let rank = match s.state {
                    SessionState::NeedsYou => 0,
                    SessionState::Working => 1,
                    SessionState::Waiting => 2,
                    SessionState::Finished { .. } => 3,
                };
                (rank, std::cmp::Reverse(s.state_since_ms.unwrap_or(0)))
            })
    })
}

/// The open tasks of every director session, by session key: the newest
/// first (last event), then by name. The monitor's rule read backwards, so
/// `[m]`, the tree's `[*]` and the monitor agree on who directs what.
pub fn directors(teams: &[TeamTask], sessions: &[Session]) -> BTreeMap<String, Vec<TaskId>> {
    let mut found: BTreeMap<String, Vec<(i64, TaskId)>> = BTreeMap::new();
    for task in teams {
        let Some(run) = run_of(task).filter(|run| !run.closed) else {
            continue;
        };
        if let Some(director) = director_of(task, &run, sessions) {
            found
                .entry(director.key.clone())
                .or_default()
                .push((run.last_ts_ms, TaskId::of(task)));
        }
    }
    found
        .into_iter()
        .map(|(key, mut tasks)| {
            tasks.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.task.cmp(&b.1.task)));
            (key, tasks.into_iter().map(|(_, id)| id).collect())
        })
        .collect()
}

/// Builds the board of a task.
pub fn board(task: &TeamTask, run: &TeamRun, sessions: &[Session], now_ms: i64) -> Board {
    let director_session = director_of(task, run, sessions);
    let director_state = director_session.map_or(SessionState::Waiting, |s| s.state);
    let mut agents = vec![Agent {
        id: USER.into(),
        title: USER.into(),
        cli: None,
        role: "usuario".into(),
        model: String::new(),
        effort: None,
        state: if director_state == SessionState::NeedsYou {
            SessionState::NeedsYou
        } else {
            SessionState::Waiting
        },
        action: if director_state == SessionState::NeedsYou {
            "te piden una decisión".into()
        } else {
            String::new()
        },
        since_ms: None,
        session: None,
        turns: 0,
        tokens: 0,
        is_director: false,
        started_at_ms: None,
        thread: None,
        writer_open: false,
        busy: false,
    }];
    let mut spans: Vec<Span> = run
        .spans
        .iter()
        .map(|(agent, start, end)| Span {
            agent: agent.clone(),
            start_ms: *start,
            end_ms: *end,
            needs_you: false,
        })
        .collect();
    let mut messages: Vec<Message> = Vec::new();
    let cli_of = |id: &str| -> Option<AgentCli> { run.find(id).map(|m| m.cli) };
    for message in &run.messages {
        messages.push(Message {
            at_ms: message.at_ms,
            from: message.from.clone(),
            to: message.to.clone(),
            text: message.text.clone(),
            sender: cli_of(&message.from),
        });
    }
    let mut feed = run.feed.clone();
    if let Some(director) = &run.director {
        let member = run.find(director);
        let since = director_session.and_then(|s| s.state_since_ms);
        let action = match director_state {
            SessionState::Working => format!("trabajando en {}", run.task),
            SessionState::NeedsYou => "te necesita".into(),
            SessionState::Waiting => "en espera".into(),
            SessionState::Finished { .. } => "sesión terminada".into(),
        };
        if matches!(
            director_state,
            SessionState::Working | SessionState::NeedsYou
        ) && let Some(since) = since
        {
            spans.push(Span {
                agent: director.clone(),
                start_ms: since,
                end_ms: None,
                needs_you: director_state == SessionState::NeedsYou,
            });
        }
        if director_state == SessionState::NeedsYou {
            let at = since.unwrap_or(now_ms);
            messages.push(Message {
                at_ms: at,
                from: director.clone(),
                to: USER.into(),
                text: "te necesita".into(),
                sender: Some(AgentCli::ClaudeCode),
            });
            feed.push(FeedItem {
                at_ms: at,
                from: Some(director.clone()),
                to: Some(USER.into()),
                text: "te necesita".into(),
            });
        }
        agents.push(Agent {
            id: director.clone(),
            title: member
                .map(|m| short_model(&m.model))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| director.clone()),
            cli: Some(AgentCli::ClaudeCode),
            role: "director".into(),
            model: member.map(|m| m.model.clone()).unwrap_or_default(),
            effort: member.and_then(|m| m.effort.clone()),
            state: director_state,
            action,
            since_ms: since,
            session: director_session.map(|s| s.key.clone()),
            turns: 0,
            tokens: 0,
            is_director: true,
            started_at_ms: director_session.and_then(|s| s.started_at_ms),
            thread: None,
            writer_open: false,
            busy: false,
        });
    }
    for member in run.workers() {
        let session = member
            .thread_id
            .as_ref()
            .map(|thread| format!("cx:{thread}"))
            .filter(|key| sessions.iter().any(|s| &s.key == key));
        let (state, action) = if run.closed {
            (
                SessionState::Finished { failed: false },
                "encargo cerrado".into(),
            )
        } else if member.working {
            (
                SessionState::Working,
                member
                    .activity
                    .clone()
                    .unwrap_or_else(|| format!("turno {}", member.turns + 1)),
            )
        } else {
            (SessionState::Waiting, "en espera".into())
        };
        agents.push(Agent {
            id: member.role.clone(),
            title: if member.model.is_empty() {
                member.role.clone()
            } else {
                short_model(&member.model)
            },
            cli: Some(member.cli),
            role: role_label(&member.role),
            model: member.model.clone(),
            effort: member.effort.clone(),
            state,
            action,
            since_ms: member.since_ms,
            session,
            turns: member.turns,
            tokens: member.tokens,
            is_director: false,
            started_at_ms: None,
            thread: member
                .thread_id
                .clone()
                .filter(|_| member.cli == AgentCli::Codex),
            writer_open: false,
            busy: member.busy,
        });
    }
    messages.sort_by_key(|m| m.at_ms);
    feed.sort_by_key(|f| f.at_ms);
    Board {
        task: run.task.clone(),
        review_role: run.review_role.clone(),
        round: run.review_round,
        max_rounds: run.max_review_rounds,
        agents,
        messages,
        feed,
        spans,
        closed: run.closed,
    }
}

impl Board {
    pub fn agent(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }

    /// Marks the agents whose thread has a terminal open in the daemon
    /// (`cx:<thread>`): the app is a writer of that thread, so the director
    /// cannot give the role a turn until it is closed.
    pub fn mark_writers(&mut self, terminals: &[TerminalRef]) {
        for agent in &mut self.agents {
            agent.writer_open = agent.thread.as_ref().is_some_and(|thread| {
                terminals
                    .iter()
                    .any(|t| t.session.strip_prefix("cx:") == Some(thread.as_str()))
            });
        }
    }

    /// The message travelling now: the director asking for the user while
    /// it needs them, otherwise the newest one sent in the last
    /// [`MESSAGE_LIVE_MS`].
    pub fn live_message(&self, now_ms: i64) -> Option<&Message> {
        let needs = self
            .agents
            .iter()
            .any(|a| a.is_director && a.state == SessionState::NeedsYou);
        if needs && let Some(ask) = self.messages.iter().rev().find(|m| m.to == USER) {
            return Some(ask);
        }
        self.messages
            .iter()
            .rev()
            .filter(|m| m.at_ms <= now_ms)
            .take_while(|m| now_ms - m.at_ms < MESSAGE_LIVE_MS)
            .next()
    }

    /// Activity of an agent in `buckets` slices of `bucket_ms` ending now,
    /// oldest first.
    pub fn histogram(&self, agent: &str, now_ms: i64, buckets: usize, bucket_ms: i64) -> Vec<Bar> {
        (0..buckets)
            .map(|i| {
                let end = now_ms - (buckets - 1 - i) as i64 * bucket_ms;
                let start = end - bucket_ms;
                let overlaps = |span: &Span| {
                    span.agent == agent
                        && span.start_ms < end
                        && span.end_ms.unwrap_or(now_ms) > start
                };
                if self.spans.iter().any(|s| s.needs_you && overlaps(s)) {
                    return Bar::NeedsYou;
                }
                if self.spans.iter().any(|s| !s.needs_you && overlaps(s)) {
                    return Bar::Working;
                }
                self.messages
                    .iter()
                    .rev()
                    .find(|m| {
                        (m.from == agent || m.to == agent) && m.at_ms >= start && m.at_ms < end
                    })
                    .map_or(Bar::Idle, |m| Bar::Message(m.sender))
            })
            .collect()
    }

    /// The sentence of «ahora»: the travelling message, else who works.
    pub fn headline(&self, now_ms: i64) -> String {
        if let Some(m) = self.live_message(now_ms) {
            return if m.to == USER {
                format!("{} te necesita", self.title(&m.from))
            } else {
                format!(
                    "{} -> {}: {}",
                    self.label(&m.from),
                    self.label(&m.to),
                    m.text
                )
            };
        }
        let working: Vec<String> = self
            .agents
            .iter()
            .filter(|a| a.state == SessionState::Working && !a.is_director)
            .map(|a| format!("{} {}", a.title, a.action))
            .collect();
        if !working.is_empty() {
            return working.join(" · ");
        }
        match self.agents.iter().find(|a| a.is_director) {
            Some(d) => format!("{} {}", d.title, d.action),
            None => "sin actividad".into(),
        }
    }

    /// Bold name of an agent id (`opus`, `sol`, `tú`).
    pub fn title(&self, id: &str) -> String {
        self.agent(id)
            .map_or_else(|| id.to_string(), |a| a.title.clone())
    }

    /// Name for a sentence: the title, with the role when two agents
    /// share it (`sol·revisor`).
    pub fn label(&self, id: &str) -> String {
        let Some(agent) = self.agent(id) else {
            return id.to_string();
        };
        let shared = self
            .agents
            .iter()
            .filter(|a| a.title == agent.title)
            .count()
            > 1;
        if shared {
            format!("{}·{}", agent.title, agent.role)
        } else {
            agent.title.clone()
        }
    }

    /// Text of one «ahora» line, with agent labels.
    pub fn feed_text(&self, item: &FeedItem) -> String {
        match (&item.from, &item.to) {
            (Some(from), Some(to)) => {
                format!("{} -> {}: {}", self.label(from), self.label(to), item.text)
            }
            (Some(from), None) => format!("{} {}", self.label(from), item.text),
            _ => item.text.clone(),
        }
    }
}

/// Root of the project a team folder belongs to.
pub fn team_project(task: &TeamTask) -> String {
    project_root(&task.folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn task(events: Vec<Value>) -> TeamTask {
        TeamTask {
            project_root: "/p".into(),
            folder: "/p/.claude/worktrees/w".into(),
            task: "demo".into(),
            events,
            created: None,
            totals: Default::default(),
            director_session: None,
        }
    }

    fn ev(kind: &str, ts: i64, fields: Value) -> Value {
        let mut value = json!({"v":1,"ts_ms":ts,"task":"demo","type":kind});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        value
    }

    fn created(ts: i64) -> Value {
        ev(
            "task_created",
            ts,
            json!({"base":"main","base_commit":"c","branch":"b","worktree":"w",
            "team":{"name":"t","director":"director","roles":[
                {"name":"director","cli":"claude","model":"opus","effort":null,"sandbox":null},
                {"name":"disenador_tests","cli":"codex","model":"gpt-6.1-sol","effort":"medium","sandbox":"workspace-write"}],
                "steps":[],"review_role":null,"max_review_rounds":1}}),
        )
    }

    fn director(state: SessionState, since: i64) -> Session {
        Session {
            key: "cl:d".into(),
            agent: AgentCli::ClaudeCode,
            cli_id: "d".into(),
            name: "director".into(),
            cwd: "/p/.claude/worktrees/w/".into(),
            project_root: "/p".into(),
            project: "p".into(),
            kind: crate::session::SessionKind::Background,
            state,
            model: Some("opus".into()),
            effort: None,
            role: None,
            parent: None,
            started_at_ms: Some(0),
            state_since_ms: Some(since),
            pid: None,
            attach: None,
            stoppable: false,
            launched_here: false,
        }
    }

    fn sample() -> (TeamTask, TeamRun) {
        let t = task(vec![
            created(1_000),
            ev(
                "brief_sent",
                2_000,
                json!({"from":"director","to":"disenador_tests","turn":1,"resumed":false,"text":"tests"}),
            ),
            ev(
                "turn_started",
                2_100,
                json!({"role":"disenador_tests","turn":1,"cli":"codex","model":"gpt-6.1-sol","thread_id":null,"review_round":null}),
            ),
            ev(
                "activity",
                2_200,
                json!({"role":"disenador_tests","turn":1,"text":"$ cargo test"}),
            ),
        ]);
        let run = latest_team(std::slice::from_ref(&t), "/p").unwrap().1;
        (t, run)
    }

    #[test]
    fn the_director_is_the_claude_session_in_the_team_folder() {
        let (t, run) = sample();
        let mut idle = director(SessionState::Waiting, 100);
        idle.key = "cl:idle".into();
        let b = board(
            &t,
            &run,
            &[idle, director(SessionState::Working, 1_500)],
            3_000,
        );
        let ids: Vec<&str> = b.agents.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, [USER, "director", "disenador_tests"]);
        let d = b.agent("director").unwrap();
        assert_eq!(
            (d.title.as_str(), d.session.as_deref()),
            ("opus", Some("cl:d"))
        );
        let worker = b.agent("disenador_tests").unwrap();
        assert_eq!(worker.state, SessionState::Working);
        assert_eq!(worker.action, "$ cargo test");
        assert_eq!(worker.title, "sol");
        assert_eq!(worker.role, "disenador tests");
        assert_eq!(b.live_message(3_000).unwrap().from, "director");
        assert!(b.live_message(2_000 + MESSAGE_LIVE_MS).is_none());
        assert_eq!(b.headline(9_000), "sol $ cargo test");
        let first = b.feed_text(&b.feed[0]);
        assert_eq!(first, "opus -> sol: tests");
    }

    #[test]
    fn a_director_that_needs_you_talks_to_the_user_until_answered() {
        let (t, run) = sample();
        let b = board(&t, &run, &[director(SessionState::NeedsYou, 2_500)], 60_000);
        assert_eq!(b.agents[0].state, SessionState::NeedsYou);
        let live = b.live_message(60_000).unwrap();
        assert_eq!((live.from.as_str(), live.to.as_str()), ("director", USER));
        assert_eq!(b.headline(60_000), "opus te necesita");
        let bars = b.histogram("director", 60_000, 4, 20_000);
        assert_eq!(bars[3], Bar::NeedsYou);
    }

    #[test]
    fn histogram_marks_work_messages_and_quiet_slices() {
        let (t, run) = sample();
        let b = board(&t, &run, &[], 10_000);
        let bars = b.histogram("disenador_tests", 10_000, 6, 1_000);
        assert_eq!(bars, vec![Bar::Working; 6]);
        let director_bars = b.histogram("director", 4_000, 3, 1_000);
        assert_eq!(
            director_bars,
            [
                Bar::Idle,
                Bar::Message(Some(AgentCli::ClaudeCode)),
                Bar::Idle
            ]
        );
        assert_eq!(b.agent("director").unwrap().state, SessionState::Waiting);
    }

    #[test]
    fn the_newest_open_task_of_the_project_is_shown() {
        let old = task(vec![created(1_000)]);
        let mut newer = task(vec![created(5_000)]);
        newer.task = "newer".into();
        let mut closed = task(vec![
            created(9_000),
            ev("task_closed", 9_500, json!({"delete_branch":false})),
        ]);
        closed.task = "closed".into();
        let teams = [old, newer, closed];
        assert_eq!(latest_team(&teams, "/p").unwrap().0.task, "newer");
        assert!(latest_team(&teams, "/other").is_none());
        assert_eq!(team_project(&teams[0]), "/p");
    }

    #[test]
    fn a_long_task_keeps_its_team_and_counters_past_the_last_events() {
        let mut long = task(vec![ev(
            "decision",
            9_000,
            json!({"by":"director","text":"sigue"}),
        )]);
        assert!(run_of(&long).is_none());
        long.created = Some(created(1_000));
        long.totals.insert(
            "disenador_tests".into(),
            crate::protocol::RoleTotals {
                turns: 7,
                tokens: 900,
            },
        );
        let run = run_of(&long).unwrap();
        assert_eq!(run.last_ts_ms, 9_000);
        let member = run.find("disenador_tests").unwrap();
        assert_eq!((member.turns, member.tokens), (7, 900));
    }

    #[test]
    fn the_session_named_by_the_events_is_the_director() {
        let (t, mut run) = sample();
        // Idle and in another folder, but it is the one titan-team named.
        let mut named = director(SessionState::Waiting, 100);
        named.key = "cl:named".into();
        named.cwd = "/elsewhere".into();
        run.director_session = Some("named".into());
        let busiest = director(SessionState::Working, 1_500);
        let b = board(&t, &run, &[busiest.clone(), named], 3_000);
        assert_eq!(
            b.agent("director").unwrap().session.as_deref(),
            Some("cl:named")
        );
        // A named session the daemon does not list falls back to the folder.
        run.director_session = Some("gone".into());
        let b = board(&t, &run, &[busiest], 3_000);
        assert_eq!(
            b.agent("director").unwrap().session.as_deref(),
            Some("cl:d")
        );
    }

    #[test]
    fn the_whole_file_names_the_director_when_the_last_events_do_not() {
        let (mut t, _) = sample();
        t.director_session = Some("whole".into());
        assert_eq!(
            run_of(&t).unwrap().director_session.as_deref(),
            Some("whole")
        );
    }
}
