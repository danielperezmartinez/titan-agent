//! Team events written by `titan-team` (format v1, docs/Decisiones/ADR-0010)
//! and the summary of one task that the monitor shows ([`TeamRun`]).
//!
//! Pure: no I/O. The tool serializes these types, the daemon forwards the raw
//! lines and the interface folds them with [`TeamRun::from_events`].

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session::AgentCli;

/// Version of the events format. Readers skip lines with a greater `v`.
pub const EVENTS_VERSION: u32 = 1;
/// Longest `text` of `brief_sent`, `activity` and `turn_finished`, in chars.
pub const TEXT_LIMIT: usize = 200;
/// Longest `text` of `decision`, in chars.
pub const DECISION_LIMIT: usize = 500;
/// Who a message goes to when there is no director: the user.
pub const USER: &str = "tú";

/// One line of `events.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamEvent {
    pub v: u32,
    pub ts_ms: i64,
    pub task: String,
    #[serde(flatten)]
    pub kind: EventKind,
}

/// The `type` of an event and its own fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    TaskCreated {
        base: String,
        base_commit: String,
        branch: String,
        worktree: String,
        team: TeamInfo,
    },
    BriefSent {
        from: Option<String>,
        to: String,
        turn: u32,
        #[serde(default)]
        resumed: bool,
        text: String,
    },
    TurnStarted {
        role: String,
        turn: u32,
        cli: String,
        model: String,
        #[serde(default)]
        effort: Option<String>,
        #[serde(default)]
        sandbox: Option<String>,
        #[serde(default)]
        thread_id: Option<String>,
        #[serde(default)]
        review_round: Option<u32>,
    },
    Activity {
        role: String,
        turn: u32,
        text: String,
    },
    TurnFinished {
        role: String,
        turn: u32,
        status: String,
        #[serde(default)]
        duration_ms: u64,
        #[serde(default)]
        thread_id: Option<String>,
        #[serde(default)]
        usage: Usage,
        #[serde(default)]
        commit: Option<String>,
        #[serde(default)]
        changed_files: Vec<String>,
        #[serde(default)]
        warnings: Vec<String>,
        #[serde(default)]
        text: String,
    },
    Report {
        role: String,
        #[serde(default)]
        review_round: Option<u32>,
        verdict: String,
        #[serde(default)]
        findings: Findings,
        #[serde(default)]
        questions: u32,
    },
    Decision {
        by: Option<String>,
        text: String,
    },
    TaskClosed {
        #[serde(default)]
        delete_branch: bool,
    },
}

/// `team` of `task_created`: the composition the task started with.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TeamInfo {
    pub name: String,
    #[serde(default)]
    pub director: Option<String>,
    #[serde(default)]
    pub roles: Vec<RoleInfo>,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub review_role: Option<String>,
    #[serde(default)]
    pub max_review_rounds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleInfo {
    pub name: String,
    pub cli: String,
    pub model: String,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub sandbox: Option<String>,
}

/// Token usage of a Codex turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

/// Findings of a review report, counted by severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Findings {
    #[serde(default)]
    pub blocker: u32,
    #[serde(default)]
    pub major: u32,
    #[serde(default)]
    pub minor: u32,
    #[serde(default)]
    pub nit: u32,
}

impl Findings {
    pub fn total(&self) -> u32 {
        self.blocker + self.major + self.minor + self.nit
    }
}

impl TeamEvent {
    pub fn new(ts_ms: i64, task: &str, kind: EventKind) -> TeamEvent {
        TeamEvent {
            v: EVENTS_VERSION,
            ts_ms,
            task: task.to_string(),
            kind,
        }
    }
}

/// Parses one event. `None` for a value that is not an event of a known
/// type, misses a common field or has a `v` greater than [`EVENTS_VERSION`].
/// Unknown fields are ignored.
pub fn parse_event(value: &Value) -> Option<TeamEvent> {
    let v = value.get("v")?.as_u64()?;
    if v == 0 || v > u64::from(EVENTS_VERSION) {
        return None;
    }
    serde_json::from_value(value.clone()).ok()
}

/// Parses one line of `events.jsonl` (see [`parse_event`]).
pub fn parse_event_line(line: &str) -> Option<TeamEvent> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    parse_event(&value)
}

/// Cuts `text` to at most `limit` chars, ending in `…` when it was longer.
pub fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out: String = text.chars().take(limit.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// First non-empty line of `text`, trimmed.
pub fn first_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

/// What the monitor shows of one task.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamRun {
    pub task: String,
    pub director: Option<String>,
    pub review_round: u32,
    pub max_review_rounds: u32,
    pub closed: bool,
    pub members: Vec<Member>,
    pub messages: Vec<TeamMessage>,
    pub last_ts_ms: i64,
    /// Every event of the run that the monitor lists in «ahora», newest
    /// last: messages, decisions and reports, already worded.
    pub feed: Vec<FeedItem>,
    /// Working spans of each member as `(role, start, end)`; `end` is
    /// `None` while the turn runs.
    pub spans: Vec<(String, i64, Option<i64>)>,
    /// Last report, as `(verdict, findings)`.
    pub last_report: Option<(String, u32)>,
}

/// One role of the team as the monitor shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub role: String,
    pub cli: AgentCli,
    pub model: String,
    pub effort: Option<String>,
    pub working: bool,
    pub turns: u32,
    pub thread_id: Option<String>,
    pub tokens: u64,
    pub activity: Option<String>,
    pub since_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamMessage {
    pub at_ms: i64,
    pub from: String,
    pub to: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedItem {
    pub at_ms: i64,
    pub text: String,
}

fn cli_of(name: &str) -> AgentCli {
    AgentCli::from_command(name).unwrap_or(AgentCli::Codex)
}

impl TeamRun {
    /// Folds the events of one task. `None` without a `task_created`.
    pub fn from_events(events: &[TeamEvent]) -> Option<TeamRun> {
        let (created_at, info) = events.iter().find_map(|event| match &event.kind {
            EventKind::TaskCreated { team, .. } => Some((event.ts_ms, team.clone())),
            _ => None,
        })?;
        let task = events
            .iter()
            .find(|e| matches!(e.kind, EventKind::TaskCreated { .. }))
            .map(|e| e.task.clone())
            .unwrap_or_default();
        let mut run = TeamRun {
            task,
            director: info.director.clone(),
            review_round: 0,
            max_review_rounds: info.max_review_rounds,
            closed: false,
            members: info
                .roles
                .iter()
                .map(|role| Member {
                    role: role.name.clone(),
                    cli: cli_of(&role.cli),
                    model: role.model.clone(),
                    effort: role.effort.clone(),
                    working: false,
                    turns: 0,
                    thread_id: None,
                    tokens: 0,
                    activity: None,
                    since_ms: None,
                })
                .collect(),
            messages: Vec::new(),
            last_ts_ms: created_at,
            feed: Vec::new(),
            spans: Vec::new(),
            last_report: None,
        };
        let director_name = info.director.clone();
        for event in events {
            run.last_ts_ms = run.last_ts_ms.max(event.ts_ms);
            let at = event.ts_ms;
            match &event.kind {
                EventKind::TaskCreated { .. } => {}
                EventKind::BriefSent { from, to, text, .. } => {
                    let from = from.clone().unwrap_or_else(|| USER.to_string());
                    run.feed.push(FeedItem {
                        at_ms: at,
                        text: format!("{from} -> {to}: {text}"),
                    });
                    run.messages.push(TeamMessage {
                        at_ms: at,
                        from,
                        to: to.clone(),
                        text: text.clone(),
                    });
                }
                EventKind::TurnStarted {
                    role,
                    thread_id,
                    review_round,
                    ..
                } => {
                    if let Some(round) = review_round {
                        run.review_round = *round;
                    }
                    let member = run.member(role);
                    member.working = true;
                    member.activity = None;
                    member.since_ms = Some(at);
                    if thread_id.is_some() {
                        member.thread_id = thread_id.clone();
                    }
                    run.spans.push((role.clone(), at, None));
                }
                EventKind::Activity { role, text, .. } => {
                    let member = run.member(role);
                    if member.working {
                        member.activity = Some(text.clone());
                    }
                }
                EventKind::TurnFinished {
                    role,
                    thread_id,
                    usage,
                    text,
                    status,
                    ..
                } => {
                    let member = run.member(role);
                    member.working = false;
                    member.activity = None;
                    member.turns += 1;
                    member.tokens += usage.input_tokens + usage.output_tokens;
                    member.since_ms = Some(at);
                    if thread_id.is_some() {
                        member.thread_id = thread_id.clone();
                    }
                    if let Some(span) = run
                        .spans
                        .iter_mut()
                        .rev()
                        .find(|(r, _, end)| r == role && end.is_none())
                    {
                        span.2 = Some(at);
                    }
                    let to = director_name.clone().unwrap_or_else(|| USER.to_string());
                    let shown = if status == "ok" || text == status {
                        text.clone()
                    } else {
                        format!("[{status}] {text}")
                    };
                    run.feed.push(FeedItem {
                        at_ms: at,
                        text: format!("{role} -> {to}: {shown}"),
                    });
                    run.messages.push(TeamMessage {
                        at_ms: at,
                        from: role.clone(),
                        to,
                        text: text.clone(),
                    });
                }
                EventKind::Report {
                    verdict, findings, ..
                } => {
                    run.last_report = Some((verdict.clone(), findings.total()));
                }
                EventKind::Decision { by, text } => {
                    let by = by.clone().unwrap_or_else(|| USER.to_string());
                    run.feed.push(FeedItem {
                        at_ms: at,
                        text: format!("{by} decide: {text}"),
                    });
                }
                EventKind::TaskClosed { .. } => {
                    run.closed = true;
                    run.feed.push(FeedItem {
                        at_ms: at,
                        text: "encargo cerrado".into(),
                    });
                }
            }
        }
        Some(run)
    }

    /// The member of `role`, added at the end if the team did not list it.
    fn member(&mut self, role: &str) -> &mut Member {
        let index = match self.members.iter().position(|m| m.role == role) {
            Some(index) => index,
            None => {
                self.members.push(Member {
                    role: role.to_string(),
                    cli: AgentCli::Codex,
                    model: String::new(),
                    effort: None,
                    working: false,
                    turns: 0,
                    thread_id: None,
                    tokens: 0,
                    activity: None,
                    since_ms: None,
                });
                self.members.len() - 1
            }
        };
        &mut self.members[index]
    }

    /// The member of a role, if the run has it.
    pub fn find(&self, role: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.role == role)
    }

    /// Members other than the director, in the team's order.
    pub fn workers(&self) -> impl Iterator<Item = &Member> {
        self.members
            .iter()
            .filter(move |m| Some(&m.role) != self.director.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn created() -> Value {
        json!({"v":1,"ts_ms":1000,"task":"demo","type":"task_created","base":"main",
            "base_commit":"abc","branch":"encargo/demo","worktree":".agents/worktrees/demo",
            "team":{"name":"t","director":"director","roles":[
                {"name":"director","cli":"claude","model":"opus","effort":null,"sandbox":null},
                {"name":"revisor","cli":"codex","model":"gpt-6.1-sol","effort":"high","sandbox":"read-only"}],
                "steps":["revisor"],"review_role":"revisor","max_review_rounds":1}})
    }

    #[test]
    fn events_round_trip_with_their_wire_names() {
        let event = parse_event(&created()).unwrap();
        let back = serde_json::to_value(&event).unwrap();
        assert_eq!(back["type"], "task_created");
        assert_eq!(back["team"]["roles"][0]["effort"], Value::Null);
        assert_eq!(parse_event(&back), Some(event));
    }

    #[test]
    fn unknown_types_newer_versions_and_junk_are_skipped() {
        assert!(parse_event(&json!({"v":1,"ts_ms":1,"task":"x","type":"rocket"})).is_none());
        let mut newer = created();
        newer["v"] = json!(2);
        assert!(parse_event(&newer).is_none());
        assert!(parse_event_line("not json").is_none());
        assert!(parse_event(&json!({"ts_ms":1,"task":"x","type":"task_closed"})).is_none());
        let extra = json!({"v":1,"ts_ms":1,"task":"x","type":"task_closed","later":true});
        assert!(parse_event(&extra).is_some());
    }

    #[test]
    fn clip_counts_chars_and_ends_with_an_ellipsis() {
        assert_eq!(clip("ñandú", 5), "ñandú");
        assert_eq!(clip("ñandúes", 5), "ñand…");
        assert_eq!(clip("abc", 5).chars().count(), 3);
    }

    #[test]
    fn a_failed_turn_says_so_in_the_feed_and_closes_its_span() {
        let mut events = vec![parse_event(&created()).unwrap()];
        for line in [
            json!({"v":1,"ts_ms":2000,"task":"demo","type":"turn_started","role":"revisor","turn":1,
                "cli":"codex","model":"gpt-6.1-sol","effort":"high","sandbox":"read-only",
                "thread_id":null,"review_round":1}),
            json!({"v":1,"ts_ms":5000,"task":"demo","type":"turn_finished","role":"revisor","turn":1,
                "status":"timeout","duration_ms":3000,"thread_id":"t-1",
                "usage":{"input_tokens":10,"cached_input_tokens":5,"output_tokens":2},
                "commit":null,"changed_files":[],"warnings":[],"text":"sin respuesta"}),
        ] {
            events.push(parse_event(&line).unwrap());
        }
        let run = TeamRun::from_events(&events).unwrap();
        assert_eq!(run.spans, vec![("revisor".into(), 2000, Some(5000))]);
        assert_eq!(run.feed.last().unwrap().text, "revisor -> director: [timeout] sin respuesta");
        assert_eq!(run.workers().count(), 1);
        assert_eq!(run.find("revisor").unwrap().tokens, 12);
    }
}
