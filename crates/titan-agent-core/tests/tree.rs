use std::collections::HashSet;
use titan_agent_core::session::{
    AgentCli, Project, Session, SessionKey, SessionKind, SessionState,
};
use titan_agent_core::tree::{Branch, TreeRow, build_tree, matches, since_label};

fn project(name: &str, root: &str) -> Project {
    Project {
        name: name.into(),
        root: root.into(),
    }
}

fn session(id: &str, name: &str, project: &Project, started: Option<i64>) -> Session {
    Session {
        key: SessionKey {
            cli: AgentCli::Codex,
            id: id.into(),
        },
        name: name.into(),
        kind: SessionKind::Interactive,
        state: SessionState::Finished { failed: false },
        cwd: "/fictional/tree/work".into(),
        project: project.clone(),
        model: None,
        parent: None,
        started_at_ms: started,
        state_since_ms: None,
        attach: None,
    }
}

fn header(project: &Project, count: usize, team_live: bool) -> TreeRow {
    TreeRow::Project {
        project: project.clone(),
        count,
        team_live,
    }
}

fn row(session: &Session, depth: usize, branch: Branch) -> TreeRow {
    TreeRow::Session {
        key: session.key.clone(),
        depth,
        branch,
    }
}

#[test]
fn branch_markers_are_exact() {
    for (branch, expected) in [
        (Branch::None, ""),
        (Branch::Middle, "|-"),
        (Branch::Last, "`-"),
    ] {
        let actual: &'static str = branch.marker();
        assert_eq!(actual, expected);
    }
}

#[test]
fn empty_input_and_unmatched_filters_have_no_rows() {
    for filter in ["", " \t\n", "missing"] {
        assert!(build_tree(&[], filter).is_empty());
    }
    let p = project("Orchid", "/fictional/orchid");
    let s = session("one", "worker", &p, None);
    assert!(build_tree(&[s], "unmatched-token").is_empty());
}

#[test]
fn project_identity_uses_both_fields_and_sorting_ignores_name_case() {
    let projects = [
        project("zeta", "/fictional/a"),
        project("BETA", "/fictional/b"),
        project("beta", "/fictional/a"),
        project("BETA", "/fictional/c"),
        project("Alpha", "/fictional/z"),
    ];
    let sessions: Vec<_> = projects
        .iter()
        .enumerate()
        .map(|(i, p)| session(&format!("s{i}"), "worker", p, None))
        .collect();
    let mut expected = Vec::new();
    for index in [4, 2, 1, 3, 0] {
        expected.push(header(&projects[index], 1, false));
        expected.push(row(&sessions[index], 0, Branch::None));
    }
    assert_eq!(build_tree(&sessions, ""), expected);
}

#[test]
fn depth_first_traversal_sorts_siblings_and_counts_finished_sessions() {
    let p = project("Orchid", "/fictional/orchid");
    let root = session("root", "root", &p, Some(0));
    let late_root = session("late-root", "root", &p, None);
    let mut early = session("early", "zeta", &p, Some(-1));
    early.parent = Some(root.key.clone());
    let mut alpha = session("alpha", "Alpha", &p, Some(5));
    alpha.parent = Some(root.key.clone());
    let mut first = session("a", "same", &p, Some(5));
    first.parent = Some(root.key.clone());
    first.key.cli = AgentCli::ClaudeCode;
    let mut second = session("a", "same", &p, Some(5));
    second.parent = Some(root.key.clone());
    let mut third = session("b", "same", &p, Some(5));
    third.parent = Some(root.key.clone());
    let mut unknown = session("unknown", "Aardvark", &p, None);
    unknown.parent = Some(root.key.clone());
    unknown.state = SessionState::Finished { failed: true };
    let mut grandchild = session("grandchild", "nested", &p, Some(-10));
    grandchild.parent = Some(alpha.key.clone());
    let sessions = vec![
        unknown.clone(),
        second.clone(),
        grandchild.clone(),
        late_root.clone(),
        third.clone(),
        alpha.clone(),
        root.clone(),
        first.clone(),
        early.clone(),
    ];
    assert_eq!(
        build_tree(&sessions, ""),
        vec![
            header(&p, 9, false),
            row(&root, 0, Branch::None),
            row(&early, 1, Branch::Middle),
            row(&alpha, 1, Branch::Middle),
            row(&grandchild, 2, Branch::Last),
            row(&first, 1, Branch::Middle),
            row(&second, 1, Branch::Middle),
            row(&third, 1, Branch::Middle),
            row(&unknown, 1, Branch::Last),
            row(&late_root, 0, Branch::None),
        ]
    );
}

#[test]
fn roots_use_the_same_start_name_and_key_order_as_children() {
    let p = project("Orchid", "/fictional/orchid");
    let early = session("early", "zeta", &p, Some(-1));
    let alpha = session("alpha", "Alpha", &p, Some(5));
    let first = session("a", "same", &p, Some(5));
    let second = session("b", "same", &p, Some(5));
    let unknown = session("unknown", "Aardvark", &p, None);
    assert_eq!(
        build_tree(
            &[
                unknown.clone(),
                second.clone(),
                alpha.clone(),
                first.clone(),
                early.clone()
            ],
            ""
        ),
        vec![
            header(&p, 5, false),
            row(&early, 0, Branch::None),
            row(&alpha, 0, Branch::None),
            row(&first, 0, Branch::None),
            row(&second, 0, Branch::None),
            row(&unknown, 0, Branch::None),
        ]
    );
}

#[test]
fn missing_parents_and_parents_in_another_project_make_roots() {
    let p = project("Orchid", "/fictional/orchid");
    let other = project("Orchid", "/fictional/other");
    let parent = session("parent", "parent", &other, None);
    let mut external = session("external", "a", &p, None);
    external.parent = Some(parent.key.clone());
    external.state = SessionState::Working;
    let mut missing = session("missing", "b", &p, None);
    missing.parent = Some(SessionKey {
        cli: AgentCli::Codex,
        id: "absent".into(),
    });
    missing.state = SessionState::Waiting;
    assert_eq!(
        build_tree(&[parent.clone(), missing.clone(), external.clone()], ""),
        vec![
            header(&p, 2, false),
            row(&external, 0, Branch::None),
            row(&missing, 0, Branch::None),
            header(&other, 1, false),
            row(&parent, 0, Branch::None),
        ]
    );
}

#[test]
fn matches_uses_only_the_declared_fields_and_trims_case_insensitively() {
    let p = project("Orchid Garden", "/fictional/root-secret");
    let mut s = session("id-secret", "Worker Amber", &p, None);
    s.model = Some("Nebula Medium".into());
    s.cwd = "/fictional/cwd-secret".into();
    s.attach = Some(vec!["attach-secret".into()]);
    for cli in [AgentCli::ClaudeCode, AgentCli::Codex] {
        s.key.cli = cli;
        for filter in [
            "",
            " \t\n",
            "  AMBER \n",
            "rKeR",
            "ORCHID",
            "garden",
            "NEBULA",
            "medium",
        ] {
            assert!(matches(&s, filter), "{cli:?}: {filter:?}");
        }
        for filter in [
            cli.tag().to_uppercase(),
            cli.command().to_uppercase(),
            cli.to_string().to_uppercase(),
        ] {
            assert!(matches(&s, &format!("  {filter}  ")));
        }
        for filter in [
            "id-secret",
            "root-secret",
            "cwd-secret",
            "attach-secret",
            "working",
            "unmatched-token",
        ] {
            assert!(!matches(&s, filter), "{filter:?}");
        }
    }
    s.model = None;
    assert!(!matches(&s, "nebula"));
    assert!(!matches(&s, "none"));
}

#[test]
fn filtering_keeps_ancestors_recomputes_branches_and_ignores_hidden_projects() {
    let p = project("Orchid", "/fictional/orchid");
    let other = project("Zinnia", "/fictional/zinnia");
    let root = session("root", "root", &p, Some(0));
    let mut hidden = session("hidden", "hidden", &p, Some(0));
    hidden.parent = Some(root.key.clone());
    hidden.state = SessionState::Working;
    let mut context = session("context", "context", &p, Some(1));
    context.parent = Some(root.key.clone());
    let mut leaf = session("leaf", "needle leaf", &p, Some(0));
    leaf.parent = Some(context.key.clone());
    let mut sibling = session("sibling", "needle sibling", &p, Some(2));
    sibling.parent = Some(root.key.clone());
    let unrelated = session("unrelated", "unrelated", &other, None);
    let sessions = vec![
        hidden.clone(),
        unrelated,
        leaf.clone(),
        sibling.clone(),
        root.clone(),
        context.clone(),
    ];
    assert_eq!(
        build_tree(&sessions, "  NeEdLe  "),
        vec![
            header(&p, 4, true),
            row(&root, 0, Branch::None),
            row(&context, 1, Branch::Middle),
            row(&leaf, 2, Branch::Last),
            row(&sibling, 1, Branch::Last),
        ]
    );
    assert_eq!(
        build_tree(&sessions, "needle leaf"),
        vec![
            header(&p, 3, true),
            row(&root, 0, Branch::None),
            row(&context, 1, Branch::Last),
            row(&leaf, 2, Branch::Last),
        ]
    );
    assert_eq!(
        build_tree(&sessions, "context"),
        vec![
            header(&p, 2, true),
            row(&root, 0, Branch::None),
            row(&context, 1, Branch::Last),
        ]
    );
    let expected = vec![
        header(&p, 5, true),
        row(&root, 0, Branch::None),
        row(&hidden, 1, Branch::Middle),
        row(&context, 1, Branch::Middle),
        row(&leaf, 2, Branch::Last),
        row(&sibling, 1, Branch::Last),
    ];
    assert_eq!(build_tree(&sessions, "  ORCHID  "), expected);
}

#[test]
fn model_and_cli_filters_are_used_by_build_tree_too() {
    let p = project("Orchid", "/fictional/orchid");
    let mut a = session("a", "first", &p, None);
    a.key.cli = AgentCli::ClaudeCode;
    a.model = Some("Nebula".into());
    let b = session("b", "second", &p, None);
    for filter in ["nebula", "cl", "claude", "claude code"] {
        assert_eq!(
            build_tree(&[a.clone(), b.clone()], filter),
            vec![header(&p, 1, false), row(&a, 0, Branch::None)]
        );
    }
    for filter in ["cx", "codex"] {
        assert_eq!(
            build_tree(&[a.clone(), b.clone()], filter),
            vec![header(&p, 1, false), row(&b, 0, Branch::None)]
        );
    }
}

#[test]
fn team_live_requires_a_live_child_with_a_parent_in_the_same_project() {
    let p = project("Orchid", "/fictional/orchid");
    let mut parent = session("parent", "parent", &p, None);
    parent.state = SessionState::Working;
    let mut child = session("child", "child", &p, None);
    child.parent = Some(parent.key.clone());
    for (state, expected) in [
        (SessionState::Working, true),
        (SessionState::NeedsYou, true),
        (SessionState::Waiting, true),
        (SessionState::Finished { failed: false }, false),
        (SessionState::Finished { failed: true }, false),
    ] {
        child.state = state;
        assert_eq!(
            build_tree(&[parent.clone(), child.clone()], "parent"),
            vec![header(&p, 1, expected), row(&parent, 0, Branch::None),]
        );
    }
}

#[test]
fn repeated_keys_keep_the_first_occurrence_even_across_projects() {
    let p = project("Orchid", "/fictional/orchid");
    let other = project("Zinnia", "/fictional/zinnia");
    let first = session("same-key", "retained", &p, Some(10));
    let duplicate = session("same-key", "discarded", &other, Some(0));
    let mut child = session("child", "child", &p, None);
    child.parent = Some(first.key.clone());
    assert_eq!(
        build_tree(&[first.clone(), duplicate.clone(), child.clone()], ""),
        vec![
            header(&p, 2, false),
            row(&first, 0, Branch::None),
            row(&child, 1, Branch::Last),
        ]
    );
    assert!(build_tree(&[first, duplicate, child], "discarded").is_empty());
}

#[test]
fn cycles_and_self_parents_emit_every_key_exactly_once() {
    let p = project("Orchid", "/fictional/orchid");
    let mut a = session("a", "a", &p, Some(0));
    let mut b = session("b", "b", &p, Some(1));
    let mut c = session("c", "c", &p, Some(2));
    let mut self_parent = session("self", "self", &p, Some(3));
    let root = session("root", "root", &p, Some(4));
    a.parent = Some(c.key.clone());
    b.parent = Some(a.key.clone());
    c.parent = Some(b.key.clone());
    self_parent.parent = Some(self_parent.key.clone());
    let sessions = vec![
        c.clone(),
        root.clone(),
        self_parent.clone(),
        b.clone(),
        a.clone(),
    ];
    let rows = build_tree(&sessions, "");
    assert_eq!(rows.first(), Some(&header(&p, 5, false)));
    assert_eq!(rows.len(), 6);
    let mut seen = HashSet::new();
    let mut stack: Vec<SessionKey> = Vec::new();
    let mut cycle_roots = 0;
    for entry in &rows[1..] {
        let TreeRow::Session { key, depth, branch } = entry else {
            panic!("unexpected project header");
        };
        assert!(seen.insert(key.clone()), "repeated key: {key:?}");
        let original = sessions
            .iter()
            .find(|s| &s.key == key)
            .expect("known session");
        if *depth == 0 {
            assert_eq!(*branch, Branch::None);
            if [a.key.clone(), b.key.clone(), c.key.clone()].contains(key) {
                cycle_roots += 1;
            }
        } else {
            assert!(*depth <= stack.len());
            assert_eq!(original.parent.as_ref(), stack.get(depth - 1));
            assert_eq!(*branch, Branch::Last);
        }
        stack.truncate(*depth);
        stack.push(key.clone());
        if key == &self_parent.key || key == &root.key {
            assert_eq!(*depth, 0);
        }
    }
    assert_eq!(cycle_roots, 1);
    assert_eq!(seen, sessions.iter().map(|s| s.key.clone()).collect());
}

#[test]
fn since_labels_use_floor_units_and_exact_boundaries() {
    let since = 123_456;
    for (elapsed, expected) in [
        (-60_000, "ahora"),
        (-1, "ahora"),
        (0, "ahora"),
        (59_999, "ahora"),
        (60_000, "hace 1 min"),
        (119_999, "hace 1 min"),
        (120_000, "hace 2 min"),
        (3_599_999, "hace 59 min"),
        (3_600_000, "hace 1 h"),
        (7_199_999, "hace 1 h"),
        (7_200_000, "hace 2 h"),
        (86_399_999, "hace 23 h"),
        (86_400_000, "hace 1 d"),
        (172_799_999, "hace 1 d"),
        (172_800_000, "hace 2 d"),
    ] {
        assert_eq!(
            since_label(since + elapsed, since),
            expected,
            "elapsed={elapsed}"
        );
        assert_eq!(since_label(-1_000_000 + elapsed, -1_000_000), expected);
    }
}

#[test]
fn whitespace_only_filter_keeps_all_rows_and_in_project_duplicates_are_ignored() {
    let p = project("Orchid", "/fictional/orchid");
    let first = session("same", "retained", &p, Some(10));
    let mut duplicate = session("same", "discarded", &p, Some(0));
    duplicate.parent = Some(first.key.clone());
    let sessions = [first.clone(), duplicate];
    let expected = vec![header(&p, 1, false), row(&first, 0, Branch::None)];
    assert_eq!(build_tree(&sessions, ""), expected);
    assert_eq!(build_tree(&sessions, " \t\n "), expected);
    assert!(build_tree(&sessions, "discarded").is_empty());
}

#[test]
fn matching_a_cycle_member_keeps_its_ancestors_without_repeating_rows() {
    let p = project("Orchid", "/fictional/orchid");
    let mut a = session("a", "needle", &p, None);
    let mut b = session("b", "context", &p, None);
    a.parent = Some(b.key.clone());
    b.parent = Some(a.key.clone());
    let unrelated = session("other", "unrelated", &p, None);
    let rows = build_tree(&[a.clone(), b.clone(), unrelated], "needle");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0], header(&p, 2, false));
    let TreeRow::Session {
        key: root,
        depth: 0,
        branch: Branch::None,
    } = &rows[1]
    else {
        panic!("cycle must have a root");
    };
    let TreeRow::Session {
        key: child,
        depth: 1,
        branch: Branch::Last,
    } = &rows[2]
    else {
        panic!("remaining cycle member must be a child");
    };
    assert_ne!(root, child);
    assert_eq!(
        HashSet::from([root.clone(), child.clone()]),
        HashSet::from([a.key, b.key])
    );
}
