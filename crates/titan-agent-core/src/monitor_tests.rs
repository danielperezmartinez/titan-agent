use super::*;
use crate::quota::Quota;
use crate::session::SessionKind;
use crate::team::events::{TeamEvent, TurnStatus, Usage};

fn role(name: &str, model: &str, label: &str, launch: bool) -> RoleInfo {
    RoleInfo {
        name: name.into(),
        label: Some(label.into()),
        cli: if launch { "codex" } else { "claude" }.into(),
        model: model.into(),
        effort: Some("high".into()),
        sandbox: None,
        purpose: None,
        launch,
    }
}

fn event(at_ms: i64, kind: EventKind) -> TeamEvent {
    TeamEvent {
        v: 1,
        at_ms,
        task: "demo".into(),
        director: Some("d1".into()),
        kind,
    }
}

fn run(events: &[TeamEvent]) -> TeamRun {
    let mut all = vec![event(
        0,
        EventKind::TaskCreated {
            branch: "encargo/demo".into(),
            base: "main".into(),
            base_dir: "/w/repo".into(),
            team: None,
            roles: vec![
                role("director", "opus", "director", false),
                role("mecanico", "gpt-6-luna", "mecánico", true),
                role("revisor", "gpt-6.1-sol", "revisor", true),
                role("disenador_tests", "gpt-6.1-sol", "tests", true),
            ],
            steps: vec!["disenador_tests".into(), "revisor".into()],
            review_role: Some("revisor".into()),
            max_review_rounds: 2,
        },
    )];
    all.extend_from_slice(events);
    TeamRun::fold("/w/repo", &all).remove(0)
}

fn started(at: i64, role: &str, message: &str) -> TeamEvent {
    event(
        at,
        EventKind::TurnStarted {
            role: role.into(),
            turn: 1,
            resumed: false,
            thread_id: Some(format!("t-{role}")),
            round: (role == "revisor").then_some(1),
            message: message.into(),
        },
    )
}

fn finished(at: i64, role: &str, summary: &str) -> TeamEvent {
    event(
        at,
        EventKind::TurnFinished {
            role: role.into(),
            turn: 1,
            status: TurnStatus::Ok,
            thread_id: Some(format!("t-{role}")),
            duration_ms: 1,
            usage: Usage {
                input_tokens: 1000,
                cached_input_tokens: 0,
                output_tokens: 500,
            },
            commit: None,
            changed_files: 1,
            warnings: 0,
            summary: summary.into(),
        },
    )
}

fn director(state: SessionState, since: i64) -> Session {
    Session {
        key: "cl:d1".into(),
        agent: AgentCli::ClaudeCode,
        cli_id: "d1".into(),
        name: "fase".into(),
        cwd: "/w/repo".into(),
        project_root: "/w/repo".into(),
        project: "repo".into(),
        kind: SessionKind::Background,
        state,
        model: Some("opus".into()),
        effort: Some("high".into()),
        role: None,
        parent: None,
        started_at_ms: None,
        state_since_ms: Some(since),
        pid: None,
        attach: None,
        stoppable: false,
        launched_here: false,
    }
}

#[test]
fn agents_follow_the_flow_and_share_names_only_when_unique() {
    let m = Monitor::build(&run(&[]), &[], &StateLog::default(), &[], 10);
    let ids: Vec<&str> = m.agents.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(
        ids,
        ["tu", "director", "disenador_tests", "revisor", "mecanico"]
    );
    let names: Vec<&str> = m.agents.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["tú", "opus", "sol·tests", "sol·revisor", "luna"]);
    assert_eq!(m.agents[2].title, "sol");
    assert_eq!(m.subtitle(), "encargo demo · ronda 1 de 2 · 4 agentes");
    assert_eq!(m.headline(), "opus crea el encargo demo");
}

#[test]
fn a_turn_is_a_message_out_a_working_lane_and_a_message_back() {
    let r = run(&[
        started(1_000, "revisor", "revisa el diff"),
        event(
            2_000,
            EventKind::Action {
                role: "revisor".into(),
                text: "lee crates/a.rs".into(),
                thread_id: None,
            },
        ),
    ]);
    let m = Monitor::build(&r, &[], &StateLog::default(), &[], 3_000);
    let sol = &m.agents[3];
    assert_eq!(sol.status, AgentStatus::Working);
    assert_eq!(sol.action, "lee crates/a.rs");
    assert_eq!(m.agents[DIRECTOR].action, "espera a sol·revisor");
    assert_eq!(m.current_message().unwrap().to, 3);
    assert_eq!(m.action_now(3, true), "recibe: revisa el diff");
    assert_eq!(
        m.action_now(DIRECTOR, false),
        "envía a sol·revisor: revisa el diff"
    );
    assert_eq!(m.segments.len(), 1);
    assert_eq!((m.segments[0].from_ms, m.segments[0].to_ms), (1_000, 3_000));

    // Later the dot is gone and the action shows on its own.
    let m = Monitor::build(&r, &[], &StateLog::default(), &[], 20_000);
    assert!(m.current_message().is_none());
    assert_eq!(
        m.talk(),
        Talk::Working {
            agent: 3,
            label: "lee crates/a.rs".into()
        }
    );
    assert_eq!(m.headline(), "sol·revisor lee crates/a.rs");

    let mut events = r.events.clone();
    events.push(finished(
        30_000,
        "revisor",
        "changes_requested · 2 hallazgos",
    ));
    events.push(event(
        30_000,
        EventKind::Review {
            role: "revisor".into(),
            round: 1,
            max_rounds: 2,
            verdict: "changes_requested".into(),
            findings: 2,
        },
    ));
    let r = TeamRun::fold("/w/repo", &events).remove(0);
    let m = Monitor::build(&r, &[], &StateLog::default(), &[], 31_000);
    assert_eq!(m.agents[3].status, AgentStatus::Waiting);
    assert_eq!(m.agents[3].turns, 1);
    assert_eq!(m.agents[3].tokens, Some(1500));
    assert_eq!(m.rounds, [RoundState::Changes, RoundState::Pending]);
    let back = m.current_message().unwrap();
    assert_eq!((back.from, back.to), (3, DIRECTOR));
    assert_eq!(
        m.feed[0].text,
        "sol·revisor -> opus: changes_requested · 2 hallazgos"
    );
    // Actions collapse in the feed: one line per run of actions.
    assert_eq!(m.feed.iter().filter(|f| f.text.contains("lee")).count(), 1);
}

#[test]
fn the_director_needs_you_from_its_session_and_its_log() {
    let r = run(&[
        started(1_000, "mecanico", "formatea"),
        finished(2_000, "mecanico", "[ok]"),
    ]);
    let mut log = StateLog::default();
    let working = director(SessionState::Working, 500);
    log.observe(std::slice::from_ref(&working), 600);
    let needs = director(SessionState::NeedsYou, 5_000);
    log.observe(std::slice::from_ref(&needs), 5_100);
    let m = Monitor::build(&r, &[needs], &log, &[], 6_000);
    assert_eq!(m.agents[DIRECTOR].status, AgentStatus::NeedsYou);
    assert_eq!(m.agents[DIRECTOR].session.as_deref(), Some("cl:d1"));
    assert_eq!(m.agents[YOU].status, AgentStatus::NeedsYou);
    assert_eq!(m.agents[YOU].action, "responde en la sesión");
    match m.talk() {
        Talk::Talking {
            from,
            to,
            needs_you,
            ..
        } => {
            assert_eq!((from, to, needs_you), (DIRECTOR, YOU, true));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(m.headline(), "opus te necesita");
    let director_segments: Vec<_> = m.segments.iter().filter(|s| s.agent == DIRECTOR).collect();
    assert_eq!(director_segments.len(), 2);
    assert!(director_segments[1].needs_you);
    let spark = m.spark(DIRECTOR, 6, 6_000);
    assert_eq!(spark[0], Spark::Working);
    assert_eq!(spark[5], Spark::NeedsYou);
    let spark = m.spark(4, 6, 6_000);
    assert_eq!(spark[1], Spark::Working);
}

#[test]
fn quotas_go_to_their_cli_and_model() {
    let quotas = vec![
        Quota {
            agent: AgentCli::Codex,
            limit: "codex".into(),
            model: None,
            window_mins: 300,
            used_percent: 20.0,
            resets_at: None,
        },
        Quota {
            agent: AgentCli::Codex,
            limit: "sol".into(),
            model: Some("gpt-6.1-sol".into()),
            window_mins: 300,
            used_percent: 90.0,
            resets_at: None,
        },
    ];
    let m = Monitor::build(&run(&[]), &[], &StateLog::default(), &quotas, 0);
    assert!(m.agents[DIRECTOR].quota.is_none(), "no Claude quota known");
    assert!(m.agents[2].quota.as_ref().unwrap().is_low());
    assert_eq!(m.agents[4].quota.as_ref().unwrap().limit, "codex");
}

#[test]
fn a_closed_task_leaves_everyone_done() {
    let r = run(&[
        started(1_000, "mecanico", "x"),
        event(2_000, EventKind::TaskClosed),
    ]);
    let m = Monitor::build(&r, &[], &StateLog::default(), &[], 3_000);
    assert!(m.agents[2..].iter().all(|a| a.status == AgentStatus::Done));
    assert!(m.segments.is_empty(), "a closed task has no open turn");
    assert_eq!(m.feed[0].text, "opus cierra el encargo");
}
