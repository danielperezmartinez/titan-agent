//! What the project monitor shows (docs/Decisiones visuales/Monitor del
//! proyecto en tres vistas.md), derived from a [`TeamRun`], the sessions and
//! the quotas. Pure: the interface only draws it, so the three views agree.
//!
//! Agents are indexed: [`YOU`] (the user), [`DIRECTOR`] and then the roles in
//! the order of the flow.

use std::collections::HashMap;

use crate::quota::{Quota, quota_for};
use crate::session::{AgentCli, Session, SessionState, short_model};
use crate::team::events::{EventKind, RoleInfo};
use crate::team::run::TeamRun;

pub const YOU: usize = 0;
pub const DIRECTOR: usize = 1;

/// How long a message stays on screen (edge lit, dot travelling, «envía»).
pub const MESSAGE_MS: i64 = 8_000;
/// One pass of the travelling dot.
pub const DOT_MS: i64 = 1_100;
/// A task with no event for this long no longer counts as a working team.
pub const IDLE_MS: i64 = 2 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Working,
    NeedsYou,
    Waiting,
    Done,
}

impl AgentStatus {
    pub fn marker(self) -> &'static str {
        match self {
            Self::Working => "[>]",
            Self::NeedsYou => "[?]",
            Self::Waiting => "[+]",
            Self::Done => "[x]",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorAgent {
    /// `tu`, or the role name in `team.toml`.
    pub id: String,
    /// Big name: `tú` or the short model (`opus`, `sol`).
    pub title: String,
    /// Role label (`director`, `tests`, `revisor`).
    pub label: String,
    /// Name in sentences, unique: the title, or `title·label` when two agents
    /// share a model.
    pub name: String,
    pub agent: Option<AgentCli>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub status: AgentStatus,
    pub since_ms: i64,
    /// What it is doing, without the message in flight.
    pub action: String,
    /// Session to open on click.
    pub session: Option<String>,
    pub turns: u32,
    /// Tokens over the task (Codex roles only).
    pub tokens: Option<u64>,
    pub quota: Option<Quota>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub at_ms: i64,
}

/// A stretch of time an agent spent working or waiting for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub agent: usize,
    pub from_ms: i64,
    pub to_ms: i64,
    pub needs_you: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedItem {
    pub at_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundState {
    Ok,
    Changes,
    InProgress,
    Pending,
}

/// The strip at the top of the control desk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Talk {
    Talking {
        from: usize,
        to: usize,
        label: String,
        at_ms: i64,
        needs_you: bool,
    },
    Working {
        agent: usize,
        label: String,
    },
    Idle,
}

/// One bar of the activity histogram of a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spark {
    Idle,
    Working,
    NeedsYou,
    /// A message sent or received; the index is the sender (its colour).
    Message(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub task: String,
    pub round: u32,
    pub max_rounds: u32,
    pub rounds: Vec<RoundState>,
    pub agents: Vec<MonitorAgent>,
    /// Oldest first.
    pub messages: Vec<Message>,
    pub segments: Vec<Segment>,
    /// Newest first.
    pub feed: Vec<FeedItem>,
    pub now_ms: i64,
}

/// States each session went through, as seen by the interface, so the lanes
/// can draw the director's past (the daemon only reports the present).
#[derive(Debug, Clone, Default)]
pub struct StateLog {
    entries: HashMap<String, Vec<(i64, SessionState)>>,
}

impl StateLog {
    const KEEP: usize = 64;

    pub fn observe(&mut self, sessions: &[Session], now_ms: i64) {
        for session in sessions {
            let since = session.state_since_ms.unwrap_or(now_ms);
            let list = self.entries.entry(session.key.clone()).or_default();
            if list.last().map(|(_, state)| *state) != Some(session.state) {
                list.push((since, session.state));
                if list.len() > Self::KEEP {
                    list.remove(0);
                }
            }
        }
    }

    /// `(from, to, state)` for one session, the last one open until `now_ms`.
    pub fn intervals(&self, key: &str, now_ms: i64) -> Vec<(i64, i64, SessionState)> {
        let Some(list) = self.entries.get(key) else {
            return Vec::new();
        };
        list.iter()
            .enumerate()
            .map(|(i, (from, state))| {
                let to = list.get(i + 1).map(|(at, _)| *at).unwrap_or(now_ms);
                (*from, to.max(*from), *state)
            })
            .collect()
    }
}

impl Monitor {
    pub fn build(
        run: &TeamRun,
        sessions: &[Session],
        log: &StateLog,
        quotas: &[Quota],
        now_ms: i64,
    ) -> Monitor {
        let session =
            |key: Option<&String>| key.and_then(|key| sessions.iter().find(|s| &s.key == key));
        let director_role = run.roles.iter().find(|r| !r.launch);
        let director_session = session(run.director_session.as_ref());
        let roles: Vec<&RoleInfo> = ordered_roles(run);

        let mut agents = vec![MonitorAgent {
            id: "tu".into(),
            title: "tú".into(),
            label: "usuario".into(),
            name: "tú".into(),
            agent: None,
            model: None,
            effort: None,
            status: AgentStatus::Waiting,
            since_ms: run.created_ms,
            action: String::new(),
            session: None,
            turns: 0,
            tokens: None,
            quota: None,
        }];
        let director_model = director_session
            .and_then(|s| s.model.clone())
            .or_else(|| director_role.map(|r| short_model(&r.model)))
            .unwrap_or_else(|| "director".into());
        agents.push(MonitorAgent {
            id: director_role.map_or("director".into(), |r| r.name.clone()),
            title: director_model.clone(),
            label: director_role
                .and_then(|r| r.label.clone())
                .unwrap_or_else(|| "director".into()),
            name: director_model.clone(),
            agent: Some(AgentCli::ClaudeCode),
            model: Some(director_model),
            effort: director_session
                .and_then(|s| s.effort.clone())
                .or_else(|| director_role.and_then(|r| r.effort.clone())),
            status: AgentStatus::Waiting,
            since_ms: run.created_ms,
            action: String::new(),
            session: director_session.map(|s| s.key.clone()),
            turns: 0,
            tokens: None,
            quota: quota_for(quotas, AgentCli::ClaudeCode, None).cloned(),
        });
        for role in &roles {
            let cli = if role.cli == "claude" {
                AgentCli::ClaudeCode
            } else {
                AgentCli::Codex
            };
            let stats = run.stats.get(&role.name).copied().unwrap_or_default();
            agents.push(MonitorAgent {
                id: role.name.clone(),
                title: short_model(&role.model),
                label: role.label.clone().unwrap_or_else(|| role.name.clone()),
                name: short_model(&role.model),
                agent: Some(cli),
                model: Some(role.model.clone()),
                effort: role.effort.clone(),
                status: if run.closed {
                    AgentStatus::Done
                } else {
                    AgentStatus::Waiting
                },
                since_ms: run.created_ms,
                action: "en espera".into(),
                session: run.sessions.get(&role.name).cloned(),
                turns: stats.turns,
                tokens: Some(stats.usage.total()),
                quota: quota_for(quotas, cli, Some(&role.model)).cloned(),
            });
        }
        disambiguate(&mut agents);
        let index: HashMap<String, usize> = agents
            .iter()
            .enumerate()
            .skip(DIRECTOR + 1)
            .map(|(i, a)| (a.id.clone(), i))
            .collect();

        let mut messages = Vec::new();
        let mut segments = Vec::new();
        let mut feed = Vec::new();
        let mut open: HashMap<usize, i64> = HashMap::new();
        let mut round_verdicts: HashMap<u32, String> = HashMap::new();
        let mut last_action_feed: Option<usize> = None;
        let dir = agents[DIRECTOR].name.clone();
        let names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
        for event in &run.events {
            let at = event.at_ms;
            let name = |role: &str| {
                index
                    .get(role)
                    .map(|&i| names[i].clone())
                    .unwrap_or_else(|| role.to_string())
            };
            let mut text = None;
            match &event.kind {
                EventKind::TaskCreated { .. } => {
                    text = Some(format!("{dir} crea el encargo {}", run.task));
                }
                EventKind::TurnStarted {
                    role,
                    message,
                    turn,
                    ..
                } => {
                    if let Some(&i) = index.get(role) {
                        messages.push(Message {
                            from: DIRECTOR,
                            to: i,
                            label: message.clone(),
                            at_ms: at,
                        });
                        open.insert(i, at);
                        agents[DIRECTOR].turns += 1;
                        agents[i].status = AgentStatus::Working;
                        agents[i].since_ms = at;
                        agents[i].action = format!("turno {turn}: {message}");
                    }
                    text = Some(format!("{dir} -> {}: {message}", name(role)));
                }
                EventKind::Action {
                    role, text: action, ..
                } => {
                    if let Some(&i) = index.get(role) {
                        agents[i].action = action.clone();
                        if last_action_feed.is_some_and(|f| f == i)
                            && let Some(previous) = feed.last_mut()
                        {
                            *previous = FeedItem {
                                at_ms: at,
                                text: format!("{} {action}", name(role)),
                            };
                            continue;
                        }
                        last_action_feed = Some(i);
                    }
                    text = Some(format!("{} {action}", name(role)));
                }
                EventKind::TurnFinished { role, summary, .. } => {
                    if let Some(&i) = index.get(role) {
                        messages.push(Message {
                            from: i,
                            to: DIRECTOR,
                            label: summary.clone(),
                            at_ms: at,
                        });
                        if let Some(from) = open.remove(&i) {
                            segments.push(Segment {
                                agent: i,
                                from_ms: from,
                                to_ms: at,
                                needs_you: false,
                            });
                        }
                        agents[i].status = AgentStatus::Waiting;
                        agents[i].since_ms = at;
                        agents[i].action = "en espera".into();
                    }
                    text = Some(format!("{} -> {dir}: {summary}", name(role)));
                }
                EventKind::Review { round, verdict, .. } => {
                    round_verdicts.insert(*round, verdict.clone());
                }
                EventKind::Note { text: note } => {
                    text = Some(format!("{dir}: {note}"));
                    agents[DIRECTOR].action = note.clone();
                }
                EventKind::TaskClosed => {
                    text = Some(format!("{dir} cierra el encargo"));
                    open.clear();
                }
            }
            if !matches!(event.kind, EventKind::Action { .. }) {
                last_action_feed = None;
            }
            if let Some(text) = text {
                feed.push(FeedItem { at_ms: at, text });
            }
        }
        for (&agent, &from) in &open {
            segments.push(Segment {
                agent,
                from_ms: from,
                to_ms: now_ms,
                needs_you: false,
            });
        }

        // The director: its session's state, and its past from the log.
        let working_role = open.keys().min().map(|&i| agents[i].name.clone());
        if let Some(director) = director_session {
            let start = run.created_ms;
            for (from, to, state) in log.intervals(&director.key, now_ms) {
                if to < start {
                    continue;
                }
                let from = from.max(start);
                match state {
                    SessionState::Working => segments.push(Segment {
                        agent: DIRECTOR,
                        from_ms: from,
                        to_ms: to,
                        needs_you: false,
                    }),
                    SessionState::NeedsYou => {
                        segments.push(Segment {
                            agent: DIRECTOR,
                            from_ms: from,
                            to_ms: to,
                            needs_you: true,
                        });
                        messages.push(Message {
                            from: DIRECTOR,
                            to: YOU,
                            label: "te necesita".into(),
                            at_ms: from,
                        });
                        feed.push(FeedItem {
                            at_ms: from,
                            text: format!("{dir} te necesita"),
                        });
                    }
                    _ => {}
                }
            }
            let d = &mut agents[DIRECTOR];
            d.since_ms = director.state_since_ms.unwrap_or(now_ms);
            d.status = match director.state {
                SessionState::Working => AgentStatus::Working,
                SessionState::NeedsYou => AgentStatus::NeedsYou,
                SessionState::Waiting => AgentStatus::Waiting,
                SessionState::Finished { .. } => AgentStatus::Done,
            };
        }
        let director_note = agents[DIRECTOR].action.clone();
        agents[DIRECTOR].action = match (agents[DIRECTOR].status, &working_role) {
            (AgentStatus::NeedsYou, _) => "te necesita".into(),
            (_, Some(role)) => format!("espera a {role}"),
            _ if !director_note.is_empty() => director_note,
            (AgentStatus::Working, None) => "trabaja en su sesión".into(),
            _ => "en espera".into(),
        };
        if agents[DIRECTOR].status == AgentStatus::NeedsYou {
            agents[YOU].status = AgentStatus::NeedsYou;
            agents[YOU].since_ms = agents[DIRECTOR].since_ms;
            agents[YOU].action = "responde en la sesión".into();
        }
        if run.closed {
            for agent in agents.iter_mut().skip(DIRECTOR) {
                agent.status = AgentStatus::Done;
                agent.action = "encargo cerrado".into();
            }
        }

        messages.sort_by_key(|m| m.at_ms);
        feed.sort_by_key(|f| std::cmp::Reverse(f.at_ms));
        let max_rounds = run.max_review_rounds;
        let rounds = (1..=max_rounds.max(run.review_round))
            .map(|n| match round_verdicts.get(&n).map(String::as_str) {
                Some("ok") => RoundState::Ok,
                Some(_) => RoundState::Changes,
                None if n <= run.review_round => RoundState::InProgress,
                None => RoundState::Pending,
            })
            .collect();
        Monitor {
            task: run.task.clone(),
            round: run.review_round,
            max_rounds,
            rounds,
            agents,
            messages,
            segments,
            feed,
            now_ms,
        }
    }

    /// The message on screen now, if any.
    pub fn current_message(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| self.now_ms - m.at_ms < MESSAGE_MS && m.at_ms <= self.now_ms)
    }

    /// Progress (0 to 1) of the dot of the message on screen.
    pub fn dot_progress(&self, message: &Message) -> f32 {
        ((self.now_ms - message.at_ms).rem_euclid(DOT_MS)) as f32 / DOT_MS as f32
    }

    /// Action of an agent with the message on screen: «envía a …» or
    /// «recibe de …» while it travels.
    pub fn action_now(&self, agent: usize, short: bool) -> String {
        if let Some(m) = self.current_message() {
            let a = &self.agents;
            if m.from == agent && agent != YOU {
                return if short {
                    format!("envía: {}", m.label)
                } else {
                    format!("envía a {}: {}", a[m.to].name, m.label)
                };
            }
            if m.to == agent && agent != YOU {
                return if short {
                    format!("recibe: {}", m.label)
                } else {
                    format!("recibe de {}: {}", a[m.from].name, m.label)
                };
            }
        }
        self.agents[agent].action.clone()
    }

    pub fn talk(&self) -> Talk {
        if let Some(m) = self.current_message() {
            return Talk::Talking {
                from: m.from,
                to: m.to,
                label: m.label.clone(),
                at_ms: m.at_ms,
                needs_you: m.to == YOU,
            };
        }
        if self.agents[DIRECTOR].status == AgentStatus::NeedsYou {
            return Talk::Talking {
                from: DIRECTOR,
                to: YOU,
                label: "te necesita".into(),
                at_ms: self.agents[DIRECTOR].since_ms,
                needs_you: true,
            };
        }
        let working = (DIRECTOR + 1..self.agents.len())
            .chain([DIRECTOR])
            .find(|&i| self.agents[i].status == AgentStatus::Working);
        match working {
            Some(agent) => Talk::Working {
                agent,
                label: self.agents[agent].action.clone(),
            },
            None => Talk::Idle,
        }
    }

    /// The sentence of «AHORA» in the lanes view.
    pub fn headline(&self) -> String {
        match self.talk() {
            Talk::Talking { from, to, .. } if to == YOU && from == DIRECTOR => {
                format!("{} te necesita", self.agents[DIRECTOR].name)
            }
            Talk::Talking {
                from, to, label, ..
            } => {
                format!(
                    "{} -> {}: {label}",
                    self.agents[from].name, self.agents[to].name
                )
            }
            Talk::Working { agent, label } => format!("{} {label}", self.agents[agent].name),
            Talk::Idle => self
                .feed
                .first()
                .map(|f| f.text.clone())
                .unwrap_or_else(|| "sin actividad".into()),
        }
    }

    /// «encargo x · ronda n de m · k agentes».
    pub fn subtitle(&self) -> String {
        let mut parts = vec![format!("encargo {}", self.task)];
        if self.max_rounds > 0 {
            parts.push(format!(
                "ronda {} de {}",
                self.round.max(1).min(self.max_rounds.max(self.round)),
                self.max_rounds.max(self.round)
            ));
        }
        parts.push(format!("{} agentes", self.agents.len() - 1));
        parts.join(" · ")
    }

    /// Activity of an agent in `buckets` slices of the last `span_ms`.
    pub fn spark(&self, agent: usize, buckets: usize, span_ms: i64) -> Vec<Spark> {
        let step = (span_ms / buckets.max(1) as i64).max(1);
        let start = self.now_ms - step * buckets as i64;
        (0..buckets)
            .map(|b| {
                let from = start + step * b as i64;
                let to = from + step;
                let overlaps = |s: &&Segment| s.agent == agent && s.from_ms < to && s.to_ms >= from;
                let segs: Vec<&Segment> = self.segments.iter().filter(overlaps).collect();
                if segs.iter().any(|s| s.needs_you) {
                    Spark::NeedsYou
                } else if !segs.is_empty() {
                    Spark::Working
                } else if let Some(m) = self
                    .messages
                    .iter()
                    .find(|m| (m.from == agent || m.to == agent) && m.at_ms >= from && m.at_ms < to)
                {
                    Spark::Message(m.from)
                } else {
                    Spark::Idle
                }
            })
            .collect()
    }
}

/// Roles shown, in the order of the flow, then the rest by name.
fn ordered_roles(run: &TeamRun) -> Vec<&RoleInfo> {
    let mut roles: Vec<&RoleInfo> = run
        .steps
        .iter()
        .filter_map(|step| run.roles.iter().find(|r| &r.name == step && r.launch))
        .collect();
    let mut rest: Vec<&RoleInfo> = run
        .roles
        .iter()
        .filter(|r| r.launch && !roles.iter().any(|x| x.name == r.name))
        .collect();
    rest.sort_by(|a, b| a.name.cmp(&b.name));
    roles.extend(rest);
    roles
}

/// Two agents with the same short model are named `model·label`.
fn disambiguate(agents: &mut [MonitorAgent]) {
    let titles: Vec<String> = agents.iter().map(|a| a.title.clone()).collect();
    for agent in agents.iter_mut().skip(DIRECTOR) {
        if titles.iter().filter(|t| **t == agent.title).count() > 1 {
            agent.name = format!("{}·{}", agent.title, agent.label);
        }
    }
}

#[cfg(test)]
#[path = "monitor_tests.rs"]
mod tests;
