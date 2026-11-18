//! The director of a task is the same for the monitor (`board`) and for the
//! session's `[m]` and the tree's `[*]` (`directors`), with two teams in one
//! project.

use serde_json::{Value, json};
use titan_agent_core::monitor::{TaskId, board, directors, run_of};
use titan_agent_core::protocol::TeamTask;
use titan_agent_core::session::{AgentCli, Session, SessionKind, SessionState};

fn task(name: &str, folder: &str, director: Option<&str>, last_ms: i64) -> TeamTask {
    let mut created = json!({"v":1,"ts_ms":1000,"task":name,"type":"task_created",
        "base":"main","base_commit":"c","branch":"b","worktree":"w",
        "team":{"name":"t","director":"director","roles":[
            {"name":"director","cli":"claude","model":"opus","effort":null,"sandbox":null}],
            "steps":[],"review_role":null,"max_review_rounds":1}});
    if let Some(id) = director {
        created["director_session"] = Value::from(id);
    }
    let last =
        json!({"v":1,"ts_ms":last_ms,"task":name,"type":"decision","by":"director","text":"sigue"});
    TeamTask {
        project_root: "/p".into(),
        folder: folder.into(),
        task: name.into(),
        events: vec![created, last],
        created: None,
        totals: Default::default(),
        director_session: None,
    }
}

fn claude(id: &str, cwd: &str, state: SessionState) -> Session {
    Session {
        key: format!("cl:{id}"),
        agent: AgentCli::ClaudeCode,
        cli_id: id.into(),
        name: id.into(),
        cwd: cwd.into(),
        project_root: "/p".into(),
        project: "p".into(),
        kind: SessionKind::Background,
        state,
        model: None,
        effort: None,
        role: None,
        parent: None,
        started_at_ms: None,
        state_since_ms: Some(0),
        pid: None,
        attach: None,
        stoppable: false,
        launched_here: false,
    }
}

#[test]
fn board_and_directors_agree_on_who_directs() {
    let sessions = vec![
        claude("a", "/p", SessionState::Waiting),
        claude("b", "/p/.claude/worktrees/x", SessionState::Working),
        claude("c", "/p", SessionState::Working),
    ];
    let teams = vec![
        task("uno", "/p", Some("a"), 5_000),
        task("dos", "/p", Some("b"), 9_000),
        // No director_session: the busiest Claude session of the folder.
        task("tres", "/p/", None, 7_000),
    ];
    let map = directors(&teams, &sessions);
    for team in &teams {
        let run = run_of(team).unwrap();
        let shown = board(team, &run, &sessions, 10_000);
        let director = shown
            .agents
            .iter()
            .find(|a| a.is_director)
            .and_then(|a| a.session.clone())
            .unwrap();
        assert!(map[&director].contains(&TaskId::of(team)), "{}", team.task);
    }
    assert_eq!(map.len(), 3);
    assert_eq!(map["cl:c"], [TaskId::of(&teams[2])]);
}
