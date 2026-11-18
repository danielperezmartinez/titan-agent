use titan_agent::conversation::folder_choices;
use titan_agent_core::protocol::ProjectInfo;
use titan_agent_core::session::{AgentCli, Session, SessionKind, SessionState};

fn project(root: &str, name: &str, missing: bool) -> ProjectInfo {
    ProjectInfo {
        root: root.into(),
        name: name.into(),
        added_ms: 1,
        missing,
    }
}
fn session(root: &str, name: &str) -> Session {
    Session {
        key: root.into(),
        agent: AgentCli::Codex,
        cli_id: root.into(),
        name: "fixture".into(),
        cwd: root.into(),
        project_root: root.into(),
        project: name.into(),
        kind: SessionKind::Exec,
        state: SessionState::Working,
        model: None,
        effort: None,
        role: None,
        parent: None,
        started_at_ms: None,
        state_since_ms: None,
        pid: None,
        attach: None,
        stoppable: false,
        launched_here: false,
    }
}

#[test]
fn draft_folders_are_ordered_by_source_and_deduplicated_without_trailing_slashes() {
    let projects = vec![
        project("/fixture/z", "Zulu", false),
        project("/fixture/b", "alpha", false),
        project("/fixture/a", "ALPHA", false),
        project("/fixture/missing", "Missing", true),
    ];
    let sessions = vec![
        session("/fixture/other-z", "zeta"),
        session("/fixture/a///", "duplicate import"),
        session("/fixture/other-b", "beta"),
        session("/fixture/other-a", "Beta"),
        session("/fixture/other-z/", "zeta"),
        session("/fixture/missing/", "Missing"),
    ];
    let picked = [
        "/fixture/other-b///",
        "/fixture/z/",
        "/fixture/picked-z",
        "/fixture/picked-a",
        "/fixture/picked-z///",
    ]
    .map(str::to_owned);
    // The contract specifies identity without trailing slashes but not which spelling
    // to emit; compare normalized paths without imposing an output spelling.
    let choices = folder_choices(&projects, &sessions, &picked);
    let normalized: Vec<_> = choices
        .into_iter()
        .map(|(path, name)| (path.trim_end_matches('/').to_owned(), name))
        .collect();
    let expected = [
        ("/fixture/a", "ALPHA"),
        ("/fixture/b", "alpha"),
        ("/fixture/z", "Zulu"),
        ("/fixture/other-a", "Beta"),
        ("/fixture/other-b", "beta"),
        ("/fixture/other-z", "zeta"),
        ("/fixture/picked-z", "picked-z"),
        ("/fixture/picked-a", "picked-a"),
    ];
    assert_eq!(
        normalized,
        expected.map(|(path, name)| (path.to_owned(), name.to_owned()))
    );
    // A missing import remains imported, so its session does not enter source 2.
    // Source 3 has no missing-folder exclusion in the literal contract.
    assert_eq!(
        folder_choices(
            &[project("/fixture/missing", "Missing", true)],
            &[],
            &["/fixture/missing".into()]
        ),
        vec![("/fixture/missing".into(), "missing".into())]
    );
    assert!(folder_choices(&[], &[], &[]).is_empty());
}
