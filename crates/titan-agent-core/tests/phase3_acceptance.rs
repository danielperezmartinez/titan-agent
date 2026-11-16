use serde_json::{Value, json};
use titan_agent_core::quota::parse_codex_rate_limits;
use titan_agent_core::session::AgentCli;
use titan_agent_core::team::{EVENTS_VERSION, TeamRun, parse_event, parse_event_line};

fn event(kind: &str, ts: i64, fields: Value) -> Value {
    let mut value = json!({"v":1,"ts_ms":ts,"task":"paper","type":kind});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    value
}

fn created() -> Value {
    event(
        "task_created",
        1000,
        json!({
            "base":"main", "base_commit":"0123456789abcdef0123456789abcdef01234567",
            "branch":"encargo/paper", "worktree":".agents/worktrees/paper",
            "team": {"name":"paper-team", "director":"director", "roles":[
                {"name":"director","cli":"claude","model":"fixture-director","effort":null,"sandbox":null},
                {"name":"reviewer","cli":"codex","model":"fixture-reviewer","effort":"high","sandbox":"read-only"}
            ], "steps":["reviewer"], "review_role":"reviewer", "max_review_rounds":3}
        }),
    )
}

fn started(ts: i64, turn: u32, round: u32, thread: Value) -> Value {
    event(
        "turn_started",
        ts,
        json!({"role":"reviewer","turn":turn,"cli":"codex",
        "model":"fixture-reviewer","effort":"high","sandbox":"read-only",
        "thread_id":thread,"review_round":round}),
    )
}

fn finished(ts: i64, turn: u32, text: &str) -> Value {
    event(
        "turn_finished",
        ts,
        json!({"role":"reviewer","turn":turn,"status":"ok",
        "duration_ms":50,"thread_id":"fixture-thread",
        "usage":{"input_tokens":10,"cached_input_tokens":8,"output_tokens":4},
        "commit":null,"changed_files":[],"warnings":[],"text":text}),
    )
}

#[test]
fn event_reader_skips_incompatible_lines_and_accepts_unknown_fields() {
    assert_eq!(EVENTS_VERSION, 1);
    let mut known = created();
    known["future_metadata"] = json!({"anything":true});
    let parsed = parse_event(&known).expect("additional fields are compatible");
    assert_eq!(parsed.ts_ms, 1000);
    assert_eq!(parsed.task, "paper");
    assert!(parse_event_line(&known.to_string()).is_some());
    assert!(parse_event_line("not JSON").is_none());
    assert!(parse_event_line("[]").is_none());
    let mut future = known.clone();
    future["v"] = json!(EVENTS_VERSION + 1);
    assert!(parse_event(&future).is_none());
    let mut unknown = known.clone();
    unknown["type"] = json!("future_event");
    assert!(parse_event(&unknown).is_none());
    for field in ["v", "ts_ms", "task", "type"] {
        let mut missing = known.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            parse_event(&missing).is_none(),
            "missing common field: {field}"
        );
    }
}

#[test]
fn team_run_reduces_turns_activity_tokens_messages_and_closure() {
    let values = vec![
        created(),
        event(
            "brief_sent",
            1010,
            json!({"from":"director","to":"reviewer","turn":1,"resumed":false,"text":"Inspect it."}),
        ),
        started(1020, 1, 1, Value::Null),
        event(
            "activity",
            1030,
            json!({"role":"reviewer","turn":1,"text":"$ inspect"}),
        ),
        finished(1040, 1, "First review."),
        started(1050, 2, 2, json!("fixture-thread")),
        event(
            "activity",
            1060,
            json!({"role":"reviewer","turn":2,"text":"edita outline.txt"}),
        ),
        finished(1070, 2, "Second review."),
        event("task_closed", 1080, json!({"delete_branch":false})),
    ];
    let events: Vec<_> = values
        .iter()
        .map(|v| parse_event(v).expect("valid fixture event"))
        .collect();
    assert!(TeamRun::from_events(&[]).is_none());
    assert!(TeamRun::from_events(&events[1..]).is_none());
    let working = TeamRun::from_events(&events[..4]).unwrap();
    assert_eq!(
        working
            .members
            .iter()
            .map(|m| m.role.as_str())
            .collect::<Vec<_>>(),
        ["director", "reviewer"]
    );
    assert_eq!(working.members[0].cli, AgentCli::ClaudeCode);
    let member = &working.members[1];
    assert_eq!(member.cli, AgentCli::Codex);
    assert_eq!(member.model, "fixture-reviewer");
    assert_eq!(member.effort.as_deref(), Some("high"));
    assert!(member.working);
    assert_eq!(member.turns, 0);
    assert_eq!(member.tokens, 0);
    assert_eq!(member.activity.as_deref(), Some("$ inspect"));
    assert_eq!(member.since_ms, Some(1020));
    assert_eq!(working.review_round, 1);
    assert!(!working.closed);
    let resumed = TeamRun::from_events(&events[..7]).unwrap();
    assert!(resumed.members[1].working);
    assert_eq!(resumed.members[1].turns, 1);
    assert_eq!(
        resumed.members[1].activity.as_deref(),
        Some("edita outline.txt")
    );
    let done = TeamRun::from_events(&events).unwrap();
    assert_eq!(done.task, "paper");
    assert_eq!(done.director.as_deref(), Some("director"));
    assert_eq!(done.max_review_rounds, 3);
    assert_eq!(done.review_round, 2);
    assert!(done.closed);
    assert_eq!(done.last_ts_ms, 1080);
    let member = &done.members[1];
    assert!(!member.working);
    assert_eq!(member.turns, 2);
    assert_eq!(member.thread_id.as_deref(), Some("fixture-thread"));
    assert_eq!(member.tokens, 28, "cached input must not be counted again");
    assert_eq!(member.activity, None);
    assert_eq!(member.since_ms, Some(1070));
    let messages: Vec<_> = done
        .messages
        .iter()
        .map(|m| (m.at_ms, m.from.as_str(), m.to.as_str(), m.text.as_str()))
        .collect();
    assert_eq!(
        messages,
        [
            (1010, "director", "reviewer", "Inspect it."),
            (1040, "reviewer", "director", "First review."),
            (1070, "reviewer", "director", "Second review.")
        ]
    );

    let mut no_director = created();
    no_director["team"]["director"] = Value::Null;
    let fallback = [
        no_director,
        event(
            "brief_sent",
            1010,
            json!({"from":null,"to":"reviewer","turn":1,"resumed":false,"text":"Inspect it."}),
        ),
        finished(1040, 1, "Reviewed."),
    ];
    let events: Vec<_> = fallback.iter().map(|v| parse_event(v).unwrap()).collect();
    let run = TeamRun::from_events(&events).unwrap();
    assert_eq!(run.messages[0].from, "tú");
    assert_eq!(run.messages[1].to, "tú");
}

#[test]
fn quota_parser_prefers_the_map_converts_seconds_and_skips_incomplete_windows() {
    let result = json!({
        "rateLimits":{"limitId":"ignored","primary":{"usedPercent":99,"windowDurationMins":1}},
        "rateLimitsByLimitId":{
            "paper": {"primary":{"usedPercent":3.5,"windowDurationMins":300,"resetsAt":1794866535},
                "secondary":{"usedPercent":19,"windowDurationMins":10080}},
            "alternate": {"limitId":"explicit","primary":{"usedPercent":2},
                "secondary":{"usedPercent":0,"windowDurationMins":60}},
            "broken":null
        }, "account":{"email":"fixture@example.invalid"},"credits":{"balance":999}
    });
    let quotas = parse_codex_rate_limits(&result);
    assert_eq!(quotas.len(), 3);
    assert!(quotas.iter().all(|q| q.agent == AgentCli::Codex));
    let paper: Vec<_> = quotas.iter().filter(|q| q.limit == "paper").collect();
    assert_eq!(paper.len(), 2);
    assert_eq!(paper[0].window_mins, 300);
    assert_eq!(paper[0].used_percent, 3.5);
    assert_eq!(paper[0].resets_at_ms, Some(1_794_866_535_000));
    assert_eq!(paper[1].window_mins, 10080);
    assert_eq!(paper[1].used_percent, 19.0);
    assert_eq!(paper[1].resets_at_ms, None);
    let alternate = quotas.iter().find(|q| q.limit == "explicit").unwrap();
    assert_eq!(alternate.window_mins, 60);
    assert_eq!(alternate.used_percent, 0.0);
    let fallback = parse_codex_rate_limits(&json!({"rateLimits":{"limitId":"codex",
        "primary":{"usedPercent":7,"windowDurationMins":300},"secondary":{"windowDurationMins":10080}}}));
    assert_eq!(fallback.len(), 1);
    assert_eq!(fallback[0].limit, "codex");
    assert_eq!(fallback[0].used_percent, 7.0);
    for malformed in [
        Value::Null,
        json!([]),
        json!(false),
        json!("bad"),
        json!({}),
    ] {
        assert!(parse_codex_rate_limits(&malformed).is_empty());
    }
    // Literal reading: an existing empty map takes precedence over rateLimits.
    assert!(
        parse_codex_rate_limits(
            &json!({"rateLimitsByLimitId":{},"rateLimits":result["rateLimits"]})
        )
        .is_empty()
    );
}
