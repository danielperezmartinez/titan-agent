//! Acceptance contract for read-only team transcripts; no production internals.
use serde_json::{Value, json};
use titan_agent_core::protocol::{
    ClientMessage, PROTOCOL_VERSION, Request, ServerMessage, TranscriptResult, decode_line,
    encode_line,
};
use titan_agent_core::team::{TeamRun, parse_event};
use titan_agent_core::transcript::{
    TRANSCRIPT_ITEMS, TRANSCRIPT_TEXT_MAX, TranscriptItem, TranscriptKind,
    transcript_from_items_page, transcript_from_thread,
};

#[test]
fn both_sources_convert_the_table_skip_empty_items_and_preserve_turn_order() {
    use TranscriptKind::*;
    let cases = vec![
        (
            json!({"type":"userMessage","content":[{"type":"text","text":"First"},{"type":"image","url":"fixture"},{"type":"text","text":"Second"}]}),
            User,
            "First\nSecond",
            None,
        ),
        (
            json!({"type":"agentMessage","text":"Reply"}),
            Agent,
            "Reply",
            None,
        ),
        (
            json!({"type":"reasoning","summary":["Consider","Conclude"]}),
            Reasoning,
            "Consider\nConclude",
            None,
        ),
        (
            json!({"type":"commandExecution","command":"echo fixture","exitCode":0}),
            Command,
            "echo fixture",
            Some("código 0"),
        ),
        (
            json!({"type":"commandExecution","command":"exit 7","exitCode":7}),
            Command,
            "exit 7",
            Some("código 7"),
        ),
        (
            json!({"type":"commandExecution","command":"pending","exitCode":null}),
            Command,
            "pending",
            None,
        ),
        (
            json!({"type":"commandExecution","command":"unknown","exitCode":"7"}),
            Command,
            "unknown",
            None,
        ),
        (
            json!({"type":"fileChange","changes":[{"path":"draft.txt"},{"path":"notes.txt"}]}),
            FileChange,
            "draft.txt, notes.txt",
            None,
        ),
        (
            json!({"type":"mcpToolCall","server":"fixture","tool":"inspect"}),
            Tool,
            "fixture.inspect",
            None,
        ),
        (
            json!({"type":"dynamicToolCall","tool":"lookup"}),
            Tool,
            "lookup",
            None,
        ),
        (
            json!({"type":"webSearch","query":"paper shapes"}),
            Tool,
            "búsqueda: paper shapes",
            None,
        ),
        (
            json!({"type":"plan","text":"Inspect the draft"}),
            Other,
            "Inspect the draft",
            None,
        ),
        (
            json!({"type":"contextCompaction"}),
            Other,
            "contextCompaction",
            None,
        ),
    ];
    let expected: Vec<_> = cases
        .iter()
        .enumerate()
        .map(|(i, (_, kind, text, detail))| TranscriptItem {
            turn: format!("turn-{i}"),
            kind: *kind,
            text: (*text).into(),
            detail: detail.map(str::to_owned),
        })
        .collect();
    let mut chronological: Vec<_> = cases
        .iter()
        .enumerate()
        .map(|(i, (item, ..))| json!({"turnId":format!("turn-{i}"),"item":item}))
        .collect();
    for item in [
        json!({}),
        json!({"type":"reasoning"}),
        json!({"type":"userMessage","content":[{"type":"image"}]}),
        json!({"type":"agentMessage","text":""}),
    ] {
        chronological.push(json!({"turnId":"empty-turn","item":item}));
    }
    chronological.push(json!({"turnId":"missing-item"}));
    let turns: Vec<_> = chronological
        .iter()
        .map(|row| json!({"id":row["turnId"],"items":[row["item"]]}))
        .collect();
    assert_eq!(
        transcript_from_thread(&json!({"thread":{"turns":turns}})),
        (expected.clone(), false)
    );
    chronological.reverse();
    for (cursor, more) in [(Value::Null, false), (json!("older-page"), true)] {
        assert_eq!(
            transcript_from_items_page(&json!({"data":chronological,"nextCursor":cursor})),
            (expected.clone(), more)
        );
    }
}

#[test]
fn transcript_limits_count_characters_and_keep_the_latest_two_hundred_items() {
    assert_eq!(TRANSCRIPT_ITEMS, 200);
    assert_eq!(TRANSCRIPT_TEXT_MAX, 4000);
    let exact = "界🦀".repeat(2000);
    let long = format!("{exact}discard");
    let expected = vec![
        TranscriptItem {
            turn: "boundary".into(),
            kind: TranscriptKind::Agent,
            text: exact.clone(),
            detail: None,
        },
        TranscriptItem {
            turn: "boundary".into(),
            kind: TranscriptKind::Agent,
            text: format!("{exact}…"),
            detail: None,
        },
    ];
    let items = json!([{"type":"agentMessage","text":exact},{"type":"agentMessage","text":long}]);
    assert_eq!(
        transcript_from_thread(&json!({"thread":{"turns":[{"id":"boundary","items":items}]}})),
        (expected.clone(), false)
    );
    assert_eq!(
        transcript_from_items_page(&json!({"data":[
        {"turnId":"boundary","item":items[1]}, {"turnId":"boundary","item":items[0]}
    ],"nextCursor":null})),
        (expected, false)
    );

    // The page contract already limits the request to 200; only thread/read
    // explicitly requires discarding older items locally.
    for count in [200, 203] {
        let turns: Vec<_> = (0..count)
            .step_by(7)
            .map(|start| {
                let items: Vec<_> = (start..(start + 7).min(count))
                    .map(|i| json!({"type":"agentMessage","text":format!("Message {i}")}))
                    .collect();
                json!({"id":format!("turn-{}", start / 7),"items":items})
            })
            .collect();
        let (items, more) = transcript_from_thread(&json!({"thread":{"turns":turns}}));
        assert_eq!(more, count > 200);
        assert_eq!(items.len(), 200);
        for (offset, item) in items.iter().enumerate() {
            let i = count - 200 + offset;
            assert_eq!(item.turn, format!("turn-{}", i / 7));
            assert_eq!(item.text, format!("Message {i}"));
        }
    }
}

#[test]
fn transcript_request_and_result_keep_the_version_one_wire_contract() {
    assert_eq!(PROTOCOL_VERSION, 1);
    let request = ClientMessage {
        id: Some(7),
        request: Request::Transcript {
            session: "cx:fixture-thread".into(),
        },
    };
    let line = encode_line(&request);
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        json!({"id":7,"method":"transcript","params":{"session":"cx:fixture-thread"}})
    );
    assert_eq!(decode_line::<ClientMessage>(&line).unwrap(), request);
    let wire = json!({"session":"cx:fixture-thread","items":[{"turn":"turn-a","kind":"file_change","text":"draft.txt"}],"more":true});
    let without_cwd: TranscriptResult = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(without_cwd.cwd, None);
    assert_eq!(without_cwd.items[0].detail, None);
    assert_eq!(serde_json::to_value(&without_cwd).unwrap(), wire);
    let with_cwd = TranscriptResult {
        cwd: Some("/fixture/project".into()),
        ..without_cwd
    };
    let message = ServerMessage::result(7, &with_cwd);
    let line = encode_line(&message);
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        json!({"id":7,"result":{
            "session":"cx:fixture-thread","items":[{"turn":"turn-a","kind":"file_change","text":"draft.txt"}],"more":true,"cwd":"/fixture/project"
        }})
    );
    assert_eq!(decode_line::<ServerMessage>(&line).unwrap(), message);
}

fn event(kind: &str, ts: i64, fields: Value) -> Value {
    let mut value = json!({"v":1,"ts_ms":ts,"task":"paper","type":kind});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    value
}

#[test]
fn busy_describes_the_last_rejected_turn_until_another_turn_starts() {
    // Open question: the available integration tests have no TeamTask, run_of,
    // board, or TerminalRef examples. Their public signatures/fields are needed
    // to extend this check to unlisted monitor threads and mark_writers without
    // guessing an API or reading production code.
    let mut values = vec![event(
        "task_created",
        1000,
        json!({
            "base":"main","base_commit":"0123456789abcdef0123456789abcdef01234567",
            "branch":"encargo/paper","worktree":".agents/worktrees/paper",
            "team":{"name":"paper-team","director":"director","roles":[
                {"name":"director","cli":"claude","model":"fixture-director","effort":null,"sandbox":null},
                {"name":"reviewer","cli":"codex","model":"fixture-reviewer","effort":null,"sandbox":"read-only"}
            ],"steps":["reviewer"],"review_role":"reviewer","max_review_rounds":3}
        }),
    )];
    let run = |values: &[Value]| {
        let events: Vec<_> = values.iter().map(|v| parse_event(v).unwrap()).collect();
        TeamRun::from_events(&events).unwrap()
    };
    assert!(!run(&values).members[1].busy);
    let start = |ts, turn| {
        event(
            "turn_started",
            ts,
            json!({"role":"reviewer","turn":turn,"cli":"codex","model":"fixture-reviewer","effort":null,"sandbox":"read-only","thread_id":"fixture-thread","review_round":1}),
        )
    };
    let finish = |ts, turn, status| {
        event(
            "turn_finished",
            ts,
            json!({"role":"reviewer","turn":turn,"status":status,"duration_ms":5,"thread_id":"fixture-thread","usage":{"input_tokens":0,"cached_input_tokens":0,"output_tokens":0},"commit":null,"changed_files":[],"warnings":[],"text":"Fixture result"}),
        )
    };
    values.push(start(1010, 1));
    values.push(finish(1020, 1, "busy"));
    assert!(run(&values).members[1].busy);
    values.push(event(
        "decision",
        1030,
        json!({"by":"director","text":"Wait"}),
    ));
    assert!(run(&values).members[1].busy);
    values.push(start(1040, 2));
    assert!(!run(&values).members[1].busy);
    values.push(finish(1050, 2, "ok"));
    assert!(!run(&values).members[1].busy);
}
