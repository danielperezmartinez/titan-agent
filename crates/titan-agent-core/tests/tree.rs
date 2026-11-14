use titan_agent_core::protocol::{Project, Session};
use titan_agent_core::session::{AgentCli, SessionState};
use titan_agent_core::tree::{TreeRow, build_tree, matches_filter};

fn session(id: &str, parent: Option<&str>, started_at: Option<i64>) -> Session {
    Session {
        id: id.into(),
        cli: AgentCli::ClaudeCode,
        name: format!("Task {id}"),
        cwd: "/work/alpha".into(),
        project: Project {
            root: "/work/alpha".into(),
            name: "Alpha".into(),
        },
        state: SessionState::Waiting,
        parent: parent.map(str::to_owned),
        pid: None,
        started_at,
        model: None,
        attach: None,
        stop: None,
    }
}

fn row(session: usize, depth: usize, branch: &str) -> TreeRow {
    TreeRow {
        session,
        depth,
        branch: branch.into(),
    }
}

#[test]
fn filter_trims_and_matches_only_the_documented_fields_case_insensitively() {
    let mut s = session("private-id", None, None);
    s.name = "Parser repair".into();
    s.cwd = "/work/hidden-directory".into();
    s.project.name = "Invented Project".into();
    s.model = Some("Model-Example".into());
    for filter in [
        "",
        "  \t",
        " PARSER ",
        "repair",
        "CLAUDE",
        "cl",
        "Claude Code",
        "PROJECT",
        "model-EXAMPLE",
    ] {
        assert!(matches_filter(&s, filter), "{filter:?}");
    }
    for filter in ["private-id", "hidden-directory", "missing"] {
        assert!(!matches_filter(&s, filter), "{filter:?}");
    }
    s.cli = AgentCli::Codex;
    s.model = None;
    for filter in ["CODEX", "CX"] {
        assert!(matches_filter(&s, filter));
    }
    assert!(!matches_filter(&s, "claude"));
    assert!(!matches_filter(&s, "model-example"));
}

#[test]
fn tree_orders_siblings_and_builds_ascii_branches_and_counts() {
    let mut sessions = vec![
        session("b", Some("root"), Some(20)),
        session("a1", Some("a"), None),
        session("root", None, Some(100)),
        session("a", Some("root"), Some(10)),
        session("c", Some("root"), None),
        session("a0", Some("a"), Some(1)),
        session("second-root", None, None),
    ];
    sessions[0].state = SessionState::Working;
    sessions[1].state = SessionState::NeedsYou;
    sessions[4].state = SessionState::Finished { failed: false };
    sessions[6].state = SessionState::Finished { failed: true };
    let groups = build_tree(&sessions, "");
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].project, sessions[0].project);
    assert_eq!(
        groups[0].rows,
        vec![
            row(2, 0, ""),
            row(3, 1, "|- "),
            row(5, 2, "|  |- "),
            row(1, 2, "|  `- "),
            row(0, 1, "|- "),
            row(4, 1, "`- "),
            row(6, 0, "")
        ]
    );
    assert_eq!(groups[0].counts.working, 1);
    assert_eq!(groups[0].counts.needs_you, 1);
    assert_eq!(groups[0].counts.waiting, 3);
    assert_eq!(groups[0].counts.total(), 5);
}

#[test]
fn last_ancestor_uses_spaces_and_equal_timestamps_use_ids() {
    let sessions = vec![
        session("root", None, None),
        session("z", Some("root"), Some(2)),
        session("a", Some("root"), Some(2)),
        session("leaf", Some("z"), None),
    ];
    assert_eq!(
        build_tree(&sessions, "")[0].rows,
        vec![
            row(0, 0, ""),
            row(2, 1, "|- "),
            row(1, 1, "`- "),
            row(3, 2, "   `- ")
        ]
    );
    let roots = vec![
        session("z", None, None),
        session("b", None, Some(2)),
        session("a", None, Some(2)),
        session("y", None, None),
    ];
    assert_eq!(
        build_tree(&roots, "")[0].rows,
        vec![row(2, 0, ""), row(1, 0, ""), row(3, 0, ""), row(0, 0, "")]
    );
}

#[test]
fn groups_use_root_identity_and_sort_by_folded_name_then_root() {
    let mut sessions = vec![
        session("one", None, None),
        session("two", Some("one"), None),
        session("three", None, None),
        session("four", None, None),
    ];
    sessions[0].project = Project {
        root: "/work/zeta".into(),
        name: "Beta".into(),
    };
    sessions[1].project = Project {
        root: "/work/beta".into(),
        name: "alpha".into(),
    };
    sessions[2].project = Project {
        root: "/work/alpha".into(),
        name: "Alpha".into(),
    };
    sessions[3].project = sessions[2].project.clone();
    let groups = build_tree(&sessions, "");
    assert_eq!(
        groups
            .iter()
            .map(|g| g.project.root.as_str())
            .collect::<Vec<_>>(),
        ["/work/alpha", "/work/beta", "/work/zeta"]
    );
    assert_eq!(groups[0].rows.len(), 2);
    assert_eq!(groups[1].rows, vec![row(1, 0, "")]);
}

#[test]
fn filtering_promotes_children_without_including_nonmatching_ancestors() {
    let mut sessions = vec![
        session("root", None, None),
        session("child", Some("root"), None),
        session("orphan", Some("absent"), None),
    ];
    sessions[1].name = "Keep this".into();
    sessions[1].state = SessionState::Working;
    let groups = build_tree(&sessions, "keep");
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].rows, vec![row(1, 0, "")]);
    assert_eq!(groups[0].counts.working, 1);
    assert_eq!(groups[0].counts.waiting, 0);
    assert!(build_tree(&sessions, "no match").is_empty());
    assert!(build_tree(&[], "").is_empty());
    assert!(build_tree(&sessions, "")[0].rows.contains(&row(2, 0, "")));
}

#[test]
fn cycles_become_roots_without_losing_or_repeating_sessions() {
    let sessions = vec![
        session("a", Some("b"), Some(1)),
        session("b", Some("a"), Some(2)),
        session("self", Some("self"), Some(3)),
        session("child", Some("a"), Some(4)),
    ];
    let groups = build_tree(&sessions, "");
    assert_eq!(groups.len(), 1);
    let rows = &groups[0].rows;
    let mut indices: Vec<_> = rows.iter().map(|r| r.session).collect();
    indices.sort_unstable();
    assert_eq!(indices, vec![0, 1, 2, 3]);
    for index in 0..3 {
        assert!(rows.contains(&row(index, 0, "")));
    }
    assert!(rows.iter().any(|r| r.session == 3 && r.depth == 1));
}

#[test]
fn nesting_is_not_limited_to_a_small_fixed_depth() {
    let sessions: Vec<_> = (0..256)
        .map(|i| {
            session(
                &format!("node-{i}"),
                if i == 0 {
                    None
                } else {
                    Some(format!("node-{}", i - 1))
                }
                .as_deref(),
                Some(i),
            )
        })
        .collect();
    let groups = build_tree(&sessions, "");
    assert_eq!(groups[0].rows.len(), sessions.len());
    for (i, r) in groups[0].rows.iter().enumerate() {
        assert_eq!(r.session, i);
        assert_eq!(r.depth, i);
        assert_eq!(
            r.branch,
            if i == 0 {
                String::new()
            } else {
                format!("{}`- ", "   ".repeat(i - 1))
            }
        );
    }
}
