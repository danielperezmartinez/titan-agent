//! Monitor acceptance contract using only public team events and models.
use serde_json::{Value, json};
use titan_agent_core::monitor::{board, run_of};
use titan_agent_core::protocol::{TeamTask, TerminalRef};

fn event(kind: &str, ts: i64, fields: Value) -> Value {
    let mut value = json!({"v":1,"ts_ms":ts,"task":"paper","type":kind});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    value
}

#[test]
fn unlisted_monitor_threads_track_only_their_own_writer_and_reflect_member_busy() {
    let mut task = TeamTask {
        project_root: "/fixture/paper-project".into(),
        folder: "/fixture/paper-project/.agents/worktrees/paper".into(),
        task: "paper".into(),
        created: None,
        totals: Default::default(),
        director_session: None,
        events: vec![
            event(
                "task_created",
                1000,
                json!({
                    "base":"main","base_commit":"0123456789abcdef0123456789abcdef01234567",
                    "branch":"encargo/paper","worktree":".agents/worktrees/paper",
                    "team":{"name":"paper-team","director":"director","roles":[
                        {"name":"director","cli":"claude","model":"fixture-director","effort":null,"sandbox":null},
                        {"name":"reviewer","cli":"codex","model":"fixture-reviewer","effort":null,"sandbox":"read-only"},
                        {"name":"auditor","cli":"codex","model":"fixture-auditor","effort":null,"sandbox":"read-only"},
                        {"name":"planner","cli":"codex","model":"fixture-planner","effort":null,"sandbox":"read-only"}
                    ],"steps":["reviewer","auditor","planner"],"review_role":"reviewer","max_review_rounds":3}
                }),
            ),
            event(
                "turn_started",
                1010,
                json!({"role":"auditor","turn":1,"cli":"codex","model":"fixture-auditor","effort":null,"sandbox":"read-only","thread_id":"fixture-auditor-thread","review_round":1}),
            ),
            event(
                "turn_finished",
                1020,
                json!({"role":"auditor","turn":1,"status":"ok","duration_ms":10,"thread_id":"fixture-auditor-thread","usage":{"input_tokens":0,"cached_input_tokens":0,"output_tokens":0},"commit":null,"changed_files":[],"warnings":[],"text":"Draft inspected"}),
            ),
            event(
                "turn_started",
                1030,
                json!({"role":"reviewer","turn":1,"cli":"codex","model":"fixture-reviewer","effort":null,"sandbox":"read-only","thread_id":"fixture-reviewer-thread","review_round":1}),
            ),
            event(
                "turn_finished",
                1040,
                json!({"role":"reviewer","turn":1,"status":"busy","duration_ms":10,"thread_id":"fixture-reviewer-thread","usage":{"input_tokens":0,"cached_input_tokens":0,"output_tokens":0},"commit":null,"changed_files":[],"warnings":[],"text":"Thread occupied"}),
            ),
        ],
    };
    let run = run_of(&task).expect("valid team event history");
    let mut monitor = board(&task, &run, &[], 1050);
    let reviewer = monitor.agent("reviewer").expect("reviewer is on the board");
    assert_eq!(reviewer.thread.as_deref(), Some("fixture-reviewer-thread"));
    assert_eq!(reviewer.session, None, "the daemon lists no sessions");
    assert_eq!(
        monitor.agent("auditor").unwrap().thread.as_deref(),
        Some("fixture-auditor-thread")
    );
    assert_eq!(monitor.agent("director").unwrap().thread, None);
    assert_eq!(monitor.agent("planner").unwrap().thread, None);
    assert!(monitor.agents.iter().all(|agent| !agent.writer_open));
    for member in &run.members {
        assert_eq!(monitor.agent(&member.role).unwrap().busy, member.busy);
    }
    assert!(reviewer.busy, "the rejected turn is visible on its agent");
    assert!(!monitor.agent("auditor").unwrap().busy);

    monitor.mark_writers(&[TerminalRef {
        session: "cx:fixture-reviewer-thread".into(),
        terminal: "fixture-terminal".into(),
    }]);
    for agent in &monitor.agents {
        assert_eq!(agent.writer_open, agent.id == "reviewer");
    }
    assert!(monitor.agent("reviewer").unwrap().busy);
    monitor.mark_writers(&[]);
    assert!(monitor.agents.iter().all(|agent| !agent.writer_open));

    task.events.push(event(
        "turn_started",
        1060,
        json!({"role":"reviewer","turn":2,"cli":"codex","model":"fixture-reviewer","effort":null,"sandbox":"read-only","thread_id":"fixture-reviewer-thread","review_round":1}),
    ));
    let run = run_of(&task).unwrap();
    let monitor = board(&task, &run, &[], 1070);
    let member = run.members.iter().find(|m| m.role == "reviewer").unwrap();
    assert!(!member.busy);
    assert_eq!(monitor.agent("reviewer").unwrap().busy, member.busy);
}
