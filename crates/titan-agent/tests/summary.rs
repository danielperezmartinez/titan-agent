//! The project summary's sessions, where a team's `[m]` leads, the draft's
//! folders without repeats and the tree with a session launched from
//! another project.

use serde_json::json;
use titan_agent::conversation::{TeamOpen, folder_choices, summary_sessions, team_open};
use titan_agent_core::monitor::TaskId;
use titan_agent_core::protocol::ProjectInfo;
use titan_agent_core::session::Session;
use titan_agent_core::tree::{NavRow, project_tree};

fn session(key: &str, root: &str, state: &str, parent: Option<&str>) -> Session {
    // `finished` carries its outcome: {"finished":{"failed":false}}.
    let state = match state {
        "finished" => json!({"finished":{"failed":false}}),
        other => json!(other),
    };
    let mut value = json!({"key":key,"agent":"claude_code","cli_id":key,"name":key,
        "cwd":root,"project_root":root,"project":root.trim_start_matches('/'),
        "kind":"background","state":state,"started_at_ms":1});
    if let Some(parent) = parent {
        value["parent"] = json!(parent);
    }
    serde_json::from_value(value).unwrap()
}

fn project(root: &str) -> ProjectInfo {
    ProjectInfo {
        root: root.into(),
        name: root.trim_start_matches('/').into(),
        added_ms: 1,
        missing: false,
    }
}

#[test]
fn the_summary_lists_the_live_sessions_of_its_project_in_tree_order() {
    let sessions = vec![
        session("cl:a", "/a", "working", None),
        session("cl:a2", "/a", "waiting", Some("cl:a")),
        session("cl:old", "/a", "finished", None),
        session("cl:b", "/b", "working", None),
    ];
    let projects = [project("/a"), project("/b")];
    assert_eq!(
        summary_sessions("/a/", &sessions, &projects, &[]),
        ["cl:a", "cl:a2"]
    );
    assert!(summary_sessions("/c", &sessions, &projects, &[]).is_empty());
    // A daemon without projects: the summary still lists them.
    assert_eq!(summary_sessions("/b", &sessions, &[], &[]), ["cl:b"]);
}

#[test]
fn a_session_from_another_project_starts_its_own_group() {
    let sessions = vec![
        session("cl:a", "/a", "working", None),
        session("cl:b", "/b", "working", Some("cl:a")),
    ];
    let rows = project_tree(&sessions, &[project("/a")], &[], "");
    assert!(rows.contains(&NavRow::Others));
    assert!(rows.contains(&NavRow::Session {
        key: "cl:b".into(),
        depth: 0,
        branch: ""
    }));
    let rows = project_tree(&sessions, &[project("/a"), project("/b")], &[], "");
    assert!(!rows.iter().any(|r| matches!(r, NavRow::Empty { .. })));
}

#[test]
fn a_team_opens_its_director_or_the_monitor_here() {
    let task = TaskId {
        folder: "/a".into(),
        task: "t".into(),
    };
    assert_eq!(
        team_open(&task, Some("cl:d")),
        TeamOpen::Director {
            session: "cl:d".into(),
            task: task.clone()
        }
    );
    assert_eq!(team_open(&task, None), TeamOpen::Here { task });
}

#[test]
fn imported_roots_are_offered_once() {
    let choices = folder_choices(&[project("/a"), project("/a/")], &[], &[]);
    assert_eq!(choices.len(), 1);
}
