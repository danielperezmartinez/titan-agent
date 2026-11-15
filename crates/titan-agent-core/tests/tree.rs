use titan_agent_core::session::{AgentCli, Session, SessionKind, SessionState};
use titan_agent_core::time::ago;
use titan_agent_core::tree::{TreeRow, build_rows};

fn session(key: &str, project: &str, parent: Option<&str>, started: i64) -> Session {
    Session {
        key: key.into(),
        agent: if key.starts_with("cx") {
            AgentCli::Codex
        } else {
            AgentCli::ClaudeCode
        },
        cli_id: key.into(),
        name: format!("name {key}"),
        cwd: format!("/p/{project}"),
        project_root: format!("/p/{project}"),
        project: project.into(),
        kind: SessionKind::Background,
        state: SessionState::Working,
        model: Some(
            if key.starts_with("cx") {
                "luna"
            } else {
                "opus"
            }
            .into(),
        ),
        effort: None,
        role: None,
        parent: parent.map(str::to_string),
        started_at_ms: Some(started),
        state_since_ms: None,
        pid: None,
        attach: None,
        stoppable: false,
    }
}

fn shape(rows: &[TreeRow]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            TreeRow::Project {
                name,
                count,
                team_live,
                ..
            } => {
                format!("# {name} {count}{}", if *team_live { " *" } else { "" })
            }
            TreeRow::Session { key, depth, branch } => format!("{depth}{branch}{key}"),
        })
        .collect()
}

#[test]
fn rows_group_by_project_and_nest_children() {
    let mut finished = session("cl:old", "kite", None, 50);
    finished.state = SessionState::Finished { failed: false };
    let sessions = vec![
        session("cl:dir", "kite", None, 10),
        session("cx:a", "kite", Some("cl:dir"), 11),
        session("cx:b", "kite", Some("cl:dir"), 12),
        session("cx:b1", "kite", Some("cx:b"), 13),
        finished,
        session("cl:new", "kite", None, 20),
        session("cl:n", "Notes", None, 5),
    ];
    assert_eq!(
        shape(&build_rows(&sessions, "")),
        vec![
            "# kite 6 *",
            "0cl:new",
            "0cl:dir",
            "1|-cx:b",
            "2`-cx:b1",
            "1`-cx:a",
            "0cl:old",
            "# Notes 1",
            "0cl:n",
        ]
    );
}

#[test]
fn filter_keeps_matches_with_their_ancestors() {
    let mut child = session("cx:a", "kite", Some("cl:dir"), 11);
    child.name = "Revisor del menú".into();
    let sessions = vec![
        session("cl:dir", "kite", None, 10),
        child,
        session("cl:other", "kite", None, 9),
        session("cl:n", "notes", None, 5),
    ];
    assert_eq!(
        shape(&build_rows(&sessions, "REVISOR")),
        vec!["# kite 2 *", "0cl:dir", "1`-cx:a"]
    );
    assert_eq!(
        shape(&build_rows(&sessions, "notes")),
        vec!["# notes 1", "0cl:n"]
    );
    assert_eq!(
        build_rows(&sessions, "luna").len(),
        3,
        "agent model matches"
    );
    assert_eq!(build_rows(&sessions, "cx").len(), 3, "agent tag matches");
    assert!(build_rows(&sessions, "zzz").is_empty());
}

#[test]
fn finished_children_do_not_mark_a_live_team() {
    let mut child = session("cx:a", "kite", Some("cl:dir"), 11);
    child.state = SessionState::Finished { failed: false };
    let rows = build_rows(&[session("cl:dir", "kite", None, 10), child], "");
    assert_eq!(shape(&rows)[0], "# kite 2");
}

#[test]
fn relative_times_are_in_spanish() {
    assert_eq!(ago(0, 5_000), "ahora");
    assert_eq!(ago(0, 42_000), "hace 42 s");
    assert_eq!(ago(0, 180_000), "hace 3 min");
    assert_eq!(ago(0, 7_200_000), "hace 2 h");
    assert_eq!(ago(0, 172_800_000), "hace 2 d");
    assert_eq!(ago(10_000, 0), "ahora");
}
