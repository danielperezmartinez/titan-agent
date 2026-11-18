use serde_json::json;
use titan_agent_core::monitor::{TaskId, director_of, directors, run_of};
use titan_agent_core::protocol::TeamTask;
use titan_agent_core::session::{AgentCli, Session, SessionKind, SessionState};

fn task(name: &str, director: Option<&str>, ts: i64) -> TeamTask {
    TeamTask {
        project_root: "/fixture/project".into(),
        folder: "/fixture/team".into(),
        task: name.into(),
        events: vec![json!({
            "v":1,"ts_ms":ts,"task":name,"type":"task_created",
            "director_session":director,
            "base":"main","base_commit":"c","branch":"b","worktree":"w",
            "team":{"name":"example","director":"director","roles":[
                {"name":"director","cli":"claude","model":"opus","effort":null,"sandbox":null},
                {"name":"disenador_tests","cli":"codex","model":"gpt-6.1-sol","effort":"medium","sandbox":"workspace-write"}
            ],"steps":[],"review_role":null,"max_review_rounds":1}
        })],
        created: None,
        totals: Default::default(),
        director_session: director.map(str::to_owned),
    }
}

fn session(id: &str, state: SessionState, since: Option<i64>) -> Session {
    Session {
        key: format!("cl:{id}"),
        agent: AgentCli::ClaudeCode,
        cli_id: id.into(),
        name: id.into(),
        cwd: "/fixture/team/".into(),
        project_root: "/fixture/project".into(),
        project: "example".into(),
        kind: SessionKind::Interactive,
        state,
        state_since_ms: since,
        model: None,
        effort: None,
        role: None,
        parent: None,
        started_at_ms: None,
        pid: None,
        attach: None,
        stoppable: false,
        launched_here: false,
    }
}

fn id(task: &TeamTask) -> TaskId {
    TaskId {
        folder: task.folder.clone(),
        task: task.task.clone(),
    }
}

#[test]
fn open_tasks_belong_to_their_explicit_directors_in_latest_event_order() {
    let sessions = vec![
        session("one", SessionState::Working, None),
        session("two", SessionState::NeedsYou, None),
        session("idle", SessionState::Waiting, None),
    ];
    let older = task("older", Some("one"), 100);
    let mut newest = task("alpha", Some("one"), 900);
    // File order defines recency, even when timestamps decrease.
    newest
        .events
        .push(json!({"v":1,"ts_ms":300,"task":"alpha","type":"brief_sent",
        "from":"director","to":"disenador_tests","turn":1,"resumed":false,"text":"tests"}));
    let tied = task("zeta", Some("one"), 300);
    let other = task("other", Some("two"), 200);
    let mut closed = task("closed", Some("one"), 400);
    closed.events.push(
        json!({"v":1,"ts_ms":500,"task":"closed","type":"task_closed","delete_branch":false}),
    );
    let mut uncreated = task("uncreated", Some("one"), 600);
    uncreated.events.clear();
    let mut no_role = task("no-role", Some("one"), 700);
    no_role.events[0]["team"]["director"] = json!(null);
    no_role.events[0]["team"]["roles"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    let map = directors(
        &[
            older.clone(),
            other.clone(),
            tied.clone(),
            closed,
            newest.clone(),
            uncreated,
            no_role,
        ],
        &sessions,
    );
    assert_eq!(map.len(), 2);
    assert_eq!(
        map.get("cl:one"),
        Some(&vec![id(&newest), id(&tied), id(&older)])
    );
    assert_eq!(map.get("cl:two"), Some(&vec![id(&other)]));
}

#[test]
fn fallback_uses_state_recency_and_input_order_with_normalized_folder_slashes() {
    let mut task = task("fallback", None, 100);
    task.folder.push_str("///");
    let sessions = vec![
        session(
            "finished",
            SessionState::Finished { failed: false },
            Some(999),
        ),
        session("waiting", SessionState::Waiting, Some(999)),
        session("working", SessionState::Working, Some(999)),
        session("unknown", SessionState::NeedsYou, None),
        session("first", SessionState::NeedsYou, Some(10)),
        session("second", SessionState::NeedsYou, Some(10)),
    ];
    let run = run_of(&task).unwrap();
    for (start, expected) in [(0, "cl:first"), (5, "cl:second")] {
        assert_eq!(
            director_of(&task, &run, &sessions[start..]).unwrap().key,
            expected
        );
    }
    for count in 1..=4 {
        assert_eq!(
            director_of(&task, &run, &sessions[..count]),
            Some(&sessions[count - 1])
        );
    }
    let mut zero = session("zero", SessionState::NeedsYou, Some(0));
    assert_eq!(
        director_of(&task, &run, &[sessions[3].clone(), zero.clone()])
            .unwrap()
            .key,
        "cl:unknown"
    );
    zero.agent = AgentCli::Codex;
    let mut elsewhere = sessions[4].clone();
    elsewhere.cwd = "/fixture/elsewhere".into();
    assert!(director_of(&task, &run, &[zero, elsewhere]).is_none());
    assert!(director_of(&task, &run, &[]).is_none());

    let mut explicit = run_of(&task).unwrap();
    explicit.director_session = Some("missing".into());
    assert_eq!(director_of(&task, &explicit, &sessions), Some(&sessions[4]));
    explicit.director_session = Some("finished".into());
    assert_eq!(director_of(&task, &explicit, &sessions), Some(&sessions[0]));
    explicit.director = None;
    assert!(director_of(&task, &explicit, &sessions).is_none());
}
