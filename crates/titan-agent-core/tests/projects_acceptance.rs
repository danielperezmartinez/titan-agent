use serde_json::json;
use titan_agent_core::monitor::{TaskId, project_teams};
use titan_agent_core::projects::{
    ImportedProject, PROJECTS_FILE, PROJECTS_VERSION, add_project, initial_projects,
    parse_projects, remove_project, render_projects,
};
use titan_agent_core::protocol::{
    Event, ProjectAddResult, ProjectInfo, Request, ServerMessage, TeamTask,
};
use titan_agent_core::session::{AgentCli, Session, SessionKind, SessionState};
use titan_agent_core::tree::{NavRow, project_tree};

fn session(key: &str, root: &str, project: &str, ts: i64) -> Session {
    Session {
        key: key.into(),
        agent: AgentCli::ClaudeCode,
        cli_id: key.into(),
        name: key.into(),
        cwd: root.into(),
        project_root: root.into(),
        project: project.into(),
        kind: SessionKind::Background,
        state: SessionState::Working,
        model: None,
        effort: None,
        role: None,
        parent: None,
        started_at_ms: Some(ts),
        state_since_ms: None,
        pid: None,
        attach: None,
        stoppable: false,
        launched_here: false,
    }
}
fn project(root: &str, name: &str, missing: bool) -> ProjectInfo {
    ProjectInfo {
        root: root.into(),
        name: name.into(),
        added_ms: 1,
        missing,
    }
}
fn task(root: &str, name: &str, director: Option<&str>, ts: i64) -> TeamTask {
    TeamTask {
        project_root: root.into(),
        folder: format!("{root}/.agents/worktrees/{name}"),
        task: name.into(),
        created: None,
        totals: Default::default(),
        director_session: director.map(str::to_owned),
        events: vec![json!({
            "v":1,"ts_ms":ts,"task":name,"type":"task_created","director_session":director,
            "base":"main","base_commit":"c","branch":"b","worktree":"w",
            "team":{"name":"fixture-team","director":"director","roles":[
                {"name":"director","cli":"claude","model":"opus","effort":null,"sandbox":null},
                {"name":"disenador_tests","cli":"codex","model":"gpt-6.1-sol","effort":"medium","sandbox":"workspace-write"}
            ],"steps":[],"review_role":"disenador_tests","max_review_rounds":2}
        })],
    }
}
fn header(root: &str, name: &str, count: usize, imported: bool, missing: bool) -> NavRow {
    NavRow::Project {
        root: root.into(),
        name: name.into(),
        count,
        imported,
        missing,
    }
}
fn row(key: &str, depth: usize, branch: &'static str) -> NavRow {
    NavRow::Session {
        key: key.into(),
        depth,
        branch,
    }
}

#[test]
fn projects_file_validates_normalizes_and_preserves_first_import() {
    assert_eq!(PROJECTS_FILE, "projects.json");
    assert_eq!(PROJECTS_VERSION, 1);
    for bad in [
        "{",
        "[]",
        "{}",
        r#"{"version":2,"projects":[]}"#,
        r#"{"version":1}"#,
        r#"{"version":1,"projects":{}}"#,
        r#"{"version":"1","projects":[]}"#,
        r#"{"version":1,"projects":[null]}"#,
        r#"{"version":1,"projects":[{"root":"/fixture/a"}]}"#,
        r#"{"version":1,"projects":[{"added_ms":1}]}"#,
        r#"{"version":1,"projects":[{"root":3,"added_ms":1}]}"#,
        r#"{"version":1,"projects":[{"root":"/fixture/a","added_ms":1.5}]}"#,
    ] {
        assert!(!parse_projects(bad).expect_err(bad).trim().is_empty());
    }
    let mut projects = parse_projects(r#"{"version":1,"extra":true,"projects":[{"root":"/fixture/z///","added_ms":10,"extra":0},{"root":"/fixture/z","added_ms":99},{"root":"/","added_ms":20}]}"#).unwrap();
    assert_eq!(
        projects,
        vec![
            ImportedProject {
                root: "/fixture/z".into(),
                added_ms: 10
            },
            ImportedProject {
                root: "/".into(),
                added_ms: 20
            }
        ]
    );
    assert_eq!(
        add_project(
            &mut projects,
            "/fixture/a/.claude/worktrees/branch/sub///",
            30
        ),
        "/fixture/a"
    );
    let before = projects.clone();
    assert_eq!(
        add_project(&mut projects, "/fixture/a/.agents/worktrees/other", 99),
        "/fixture/a"
    );
    assert_eq!(projects, before);
    assert_eq!(
        projects.last().unwrap(),
        &ImportedProject {
            root: "/fixture/a".into(),
            added_ms: 30
        }
    );
    let rendered = render_projects(&projects);
    assert!(rendered.ends_with('\n'));
    assert_eq!(parse_projects(&rendered), Ok(projects.clone()));
    assert!(remove_project(&mut projects, "/fixture/z///"));
    assert!(!remove_project(&mut projects, "/fixture/z"));
    assert_eq!(projects, before[1..]);
    let sessions = vec![
        session("z", "/fixture/z/", "z", 1),
        session("a", "/fixture/a/.agents/worktrees/x", "a", 2),
        session("duplicate", "/fixture/a", "a", 3),
    ];
    assert_eq!(
        initial_projects(&sessions, 40),
        vec![
            ImportedProject {
                root: "/fixture/a".into(),
                added_ms: 40
            },
            ImportedProject {
                root: "/fixture/z".into(),
                added_ms: 40
            }
        ]
    );
}

#[test]
fn project_protocol_round_trips_and_accepts_legacy_sessions() {
    for (request, wire) in [
        (
            Request::ProjectAdd {
                path: "/fixture/a".into(),
            },
            json!({"method":"project_add","params":{"path":"/fixture/a"}}),
        ),
        (
            Request::ProjectRemove {
                root: "/fixture/a".into(),
            },
            json!({"method":"project_remove","params":{"root":"/fixture/a"}}),
        ),
    ] {
        assert_eq!(serde_json::to_value(&request).unwrap(), wire);
        assert_eq!(serde_json::from_value::<Request>(wire).unwrap(), request);
    }
    let result = ProjectAddResult {
        root: "/fixture/a".into(),
    };
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({"root":"/fixture/a"})
    );
    assert_eq!(
        serde_json::from_value::<ProjectAddResult>(json!({"root":"/fixture/a"})).unwrap(),
        result
    );
    let legacy = json!({"event":"sessions","params":{"sessions":[],"sources":[],"terminals":[],"launches":[],"teams":[],"quotas":[]}});
    let message: ServerMessage = serde_json::from_value(legacy.clone()).unwrap();
    assert!(matches!(
        &message,
        ServerMessage::Event(Event::Sessions {
            projects: None,
            projects_error: None,
            ..
        })
    ));
    assert_eq!(serde_json::to_value(message).unwrap(), legacy);
    let mut modern = legacy;
    modern["params"]["projects"] =
        json!([{"root":"/fixture/a","name":"a","added_ms":12,"missing":true}]);
    modern["params"]["projects_error"] = json!("fixture error");
    let message: ServerMessage = serde_json::from_value(modern.clone()).unwrap();
    assert_eq!(serde_json::to_value(message).unwrap(), modern);
}

#[test]
fn tree_orders_imports_by_session_and_last_team_event_then_other_folders() {
    let mut active = session("active", "/fixture/b///", "Beta", 20);
    active.state_since_ms = Some(700);
    let sessions = vec![
        session("outside", "/fixture/outside", "Outside", 9999),
        active,
    ];
    let projects = vec![
        project("/fixture/z", "Zulu", false),
        project("/fixture/a", "Alpha", true),
        project("/fixture/b", "Beta", false),
        project("/fixture/c", "Charlie", false),
    ];
    let mut team = task("/fixture/c", "activity", None, 10000);
    // The contract says the last event, not the maximum timestamp in its history.
    team.events.push(json!({"v":1,"ts_ms":600,"task":"activity","type":"brief_sent","from":"director","to":"disenador_tests","turn":1,"resumed":false,"text":"fixture"}));
    assert_eq!(
        project_tree(&sessions, &projects, &[team], ""),
        vec![
            header("/fixture/b", "Beta", 1, true, false),
            row("active", 0, ""),
            header("/fixture/c", "Charlie", 0, true, false),
            NavRow::Empty {
                root: "/fixture/c".into()
            },
            header("/fixture/a", "Alpha", 0, true, true),
            NavRow::Empty {
                root: "/fixture/a".into()
            },
            header("/fixture/z", "Zulu", 0, true, false),
            NavRow::Empty {
                root: "/fixture/z".into()
            },
            NavRow::Others,
            header("/fixture/outside", "Outside", 1, false, false),
            row("outside", 0, "")
        ]
    );
}

#[test]
fn tree_filters_both_groups_retaining_ancestors_and_whole_named_projects() {
    let mut parent = session("parent", "/fixture/a", "Alpha", 10);
    parent.state = SessionState::Finished { failed: false };
    let mut child = session("needle-child", "/fixture/a", "Alpha", 20);
    child.parent = Some("parent".into());
    let mut outside = session("outside", "/fixture/o", "Other", 30);
    outside.model = Some("Needle-model".into());
    let sessions = vec![
        parent,
        child,
        session("unmatched", "/fixture/a", "Alpha", 40),
        outside,
    ];
    let projects = vec![
        project("/fixture/a", "Alpha", false),
        project("/fixture/e", "Needle Empty", false),
    ];
    assert_eq!(
        project_tree(&sessions, &projects, &[], "  nEEdle  "),
        vec![
            header("/fixture/a", "Alpha", 2, true, false),
            row("parent", 0, ""),
            row("needle-child", 1, "`-"),
            header("/fixture/e", "Needle Empty", 0, true, false),
            NavRow::Empty {
                root: "/fixture/e".into()
            },
            NavRow::Others,
            header("/fixture/o", "Other", 1, false, false),
            row("outside", 0, "")
        ]
    );
    assert_eq!(
        project_tree(&sessions, &projects, &[], "alpha"),
        vec![
            header("/fixture/a", "Alpha", 3, true, false),
            row("unmatched", 0, ""),
            row("parent", 0, ""),
            row("needle-child", 1, "`-")
        ]
    );
    assert!(project_tree(&sessions, &projects, &[], "absent").is_empty());
}

#[test]
fn project_summary_keeps_open_tasks_with_distinct_and_unknown_directors() {
    let mut first = task("/fixture/a", "alpha", Some("one"), 100);
    first.events.push(json!({"v":1,"ts_ms":200,"task":"alpha","type":"brief_sent","from":"director","to":"disenador_tests","turn":1,"resumed":false,"text":"tests"}));
    first.events.push(json!({"v":1,"ts_ms":210,"task":"alpha","type":"turn_started","role":"disenador_tests","turn":1,"cli":"codex","model":"gpt-6.1-sol"}));
    let second = task("/fixture/a", "beta", Some("two"), 300);
    let unknown = task("/fixture/a", "gamma", None, 300);
    let mut closed = task("/fixture/a", "closed", Some("one"), 400);
    closed.events.push(
        json!({"v":1,"ts_ms":500,"task":"closed","type":"task_closed","delete_branch":false}),
    );
    let mut one = session("cl:one", "/fixture/a", "Alpha", 1);
    one.cli_id = "one".into();
    one.cwd = first.folder.clone();
    let mut two = session("cl:two", "/fixture/a", "Alpha", 2);
    two.cli_id = "two".into();
    two.cwd = second.folder.clone();
    let mut uncreated = task("/fixture/a", "uncreated", None, 900);
    uncreated.events.clear();
    let tasks = vec![
        first,
        second,
        unknown,
        closed,
        uncreated,
        task("/fixture/elsewhere", "foreign", None, 1000),
    ];
    let lines = project_teams("/fixture/a///", &tasks, &[one, two], 310);
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.id.clone(), line.director.as_deref(), line.last_ts_ms))
            .collect::<Vec<_>>(),
        vec![
            (
                TaskId {
                    folder: tasks[1].folder.clone(),
                    task: "beta".into()
                },
                Some("cl:two"),
                300
            ),
            (
                TaskId {
                    folder: tasks[2].folder.clone(),
                    task: "gamma".into()
                },
                None,
                300
            ),
            (
                TaskId {
                    folder: tasks[0].folder.clone(),
                    task: "alpha".into()
                },
                Some("cl:one"),
                210
            )
        ]
    );
    assert_eq!(lines[2].state, SessionState::Working);
    assert_eq!(lines[2].max_rounds, 2);
    // Headline wording and round derivation belong to board; their fixtures/signatures
    // are not supplied by this contract, so no new wording is assumed here.
}
