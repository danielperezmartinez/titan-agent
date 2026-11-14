use serde_json::json;
use titan_agent_core::discovery::{
    claude_state, codex_state, fallback_project, project_from_git_common_dir, project_name,
    session_from_codex_thread, sessions_from_claude_agents,
};
use titan_agent_core::protocol::{Project, StopAction};
use titan_agent_core::session::{AgentCli, SessionState};

fn project(cwd: &str) -> Project {
    Project {
        root: cwd.to_owned(),
        name: "Invented project".into(),
    }
}

#[test]
fn claude_state_obeys_explicit_state_before_status() {
    for has_pid in [false, true] {
        for status in [None, Some("busy"), Some("idle"), Some("other")] {
            for (state, expected) in [
                ("working", SessionState::Working),
                ("blocked", SessionState::NeedsYou),
                (
                    "done",
                    if has_pid {
                        SessionState::Waiting
                    } else {
                        SessionState::Finished { failed: false }
                    },
                ),
                ("stopped", SessionState::Finished { failed: false }),
                ("failed", SessionState::Finished { failed: true }),
                ("error", SessionState::Finished { failed: true }),
            ] {
                assert_eq!(claude_state(Some(state), status, has_pid), expected);
            }
            for state in [None, Some("unknown")] {
                assert_eq!(
                    claude_state(state, status, has_pid),
                    if status == Some("busy") {
                        SessionState::Working
                    } else {
                        SessionState::Waiting
                    }
                );
            }
        }
    }
}

#[test]
fn claude_translation_preserves_order_metadata_and_capabilities() {
    let input = json!([
        {"id":"abcdefgh-extra", "sessionId":"ignored-thread", "cwd":"/work/alpha", "name":"alpha task", "pid":4242, "startedAt":1700000000123_i64, "state":"blocked", "kind":"background", "extra":true},
        {"sessionId":"ijklmnop-extra", "cwd":"/work/beta", "name":"", "pid":4243, "status":"idle", "kind":"interactive"},
        {"id":"tiny", "cwd":"/work/gamma", "state":"done"},
        {"cwd":"/work/omitted"}, {"id":"missing-cwd"}, {}
    ]);
    let sessions = sessions_from_claude_agents(&input.to_string(), &project).unwrap();
    assert_eq!(sessions.len(), 3);
    let first = &sessions[0];
    assert_eq!(first.id, "claude:abcdefgh-extra");
    assert_eq!(first.cli, AgentCli::ClaudeCode);
    assert_eq!(first.name, "alpha task");
    assert_eq!(first.cwd, "/work/alpha");
    assert_eq!(first.project, project("/work/alpha"));
    assert_eq!(first.state, SessionState::NeedsYou);
    assert_eq!(first.pid, Some(4242));
    assert_eq!(first.started_at, Some(1700000000123));
    assert_eq!(
        first.attach,
        Some(vec![
            "claude".into(),
            "attach".into(),
            "abcdefgh-extra".into()
        ])
    );
    assert_eq!(first.stop, Some(StopAction::Stop));
    let interactive = &sessions[1];
    assert_eq!(interactive.id, "claude:ijklmnop-extra");
    assert_eq!(interactive.name, "ijklmnop");
    assert_eq!(interactive.pid, Some(4243));
    assert_eq!(interactive.state, SessionState::Waiting);
    assert_eq!(interactive.attach, None);
    assert_eq!(interactive.stop, None);
    assert_eq!(interactive.started_at, None);
    assert_eq!(sessions[2].name, "tiny");
    assert_eq!(sessions[2].pid, None);
    assert_eq!(sessions[2].stop, None);
    assert_eq!(sessions[2].state, SessionState::Finished { failed: false });
    for session in &sessions {
        assert_eq!(session.parent, None);
        assert_eq!(session.model, None);
        assert_eq!(session.project, project(&session.cwd));
    }
}

#[test]
fn claude_requires_a_json_array_and_accepts_an_empty_array() {
    assert!(
        sessions_from_claude_agents("[]", &project)
            .unwrap()
            .is_empty()
    );
    for input in ["{", "{}", "null", "42", "\"array\""] {
        assert!(sessions_from_claude_agents(input, &project).is_err());
    }
}

#[test]
fn codex_status_maps_activity_flags_and_terminal_states() {
    for (status, expected) in [
        (json!({"type":"active"}), SessionState::Working),
        (
            json!({"type":"active","activeFlags":[]}),
            SessionState::Working,
        ),
        (
            json!({"type":"active","activeFlags":["waitingOnApproval"]}),
            SessionState::NeedsYou,
        ),
        (
            json!({"type":"active","activeFlags":["inventedFlag"]}),
            SessionState::NeedsYou,
        ),
        (json!({"type":"idle"}), SessionState::Waiting),
        (
            json!({"type":"systemError"}),
            SessionState::Finished { failed: true },
        ),
        (
            json!({"type":"notLoaded"}),
            SessionState::Finished { failed: false },
        ),
        (
            json!({"type":"unknown"}),
            SessionState::Finished { failed: false },
        ),
        (json!(null), SessionState::Finished { failed: false }),
    ] {
        assert_eq!(codex_state(&status), expected);
    }
}

#[test]
fn codex_translation_preserves_metadata_and_derives_stop_from_state() {
    for (status, stop) in [
        (json!({"type":"active"}), Some(StopAction::Interrupt)),
        (
            json!({"type":"active","activeFlags":["waitingOnApproval"]}),
            Some(StopAction::Interrupt),
        ),
        (json!({"type":"idle"}), None),
        (json!({"type":"systemError"}), None),
        (json!(null), None),
    ] {
        let thread = json!({"id":"thread-alpha", "cwd":"/work/alpha", "name":"Named task", "preview":"ignored", "createdAt":1700000000, "model":"gpt-invented", "parentThreadId":"thread-parent", "status":status, "extra":true});
        let session = session_from_codex_thread(&thread, &project).unwrap();
        assert_eq!(session.id, "codex:thread-alpha");
        assert_eq!(session.cli, AgentCli::Codex);
        assert_eq!(session.name, "Named task");
        assert_eq!(session.cwd, "/work/alpha");
        assert_eq!(session.project, project("/work/alpha"));
        assert_eq!(session.parent.as_deref(), Some("codex:thread-parent"));
        assert_eq!(session.started_at, Some(1700000000000));
        assert_eq!(session.model.as_deref(), Some("gpt-invented"));
        assert_eq!(session.pid, None);
        assert_eq!(session.state, codex_state(&thread["status"]));
        assert_eq!(
            session.attach,
            Some(vec!["codex".into(), "resume".into(), "thread-alpha".into()])
        );
        assert_eq!(session.stop, stop);
    }
}

#[test]
fn codex_names_use_first_preview_line_unicode_limit_and_id_fallback() {
    for (name, preview, expected) in [
        (
            json!(null),
            json!("First line\nSecond line"),
            "First line".to_owned(),
        ),
        (json!(""), json!("界".repeat(61)), "界".repeat(60)),
        (json!(42), json!(""), "abcdefgh".to_owned()),
        (json!(null), json!("\nSecond line"), "abcdefgh".to_owned()),
        (json!(null), json!(null), "abcdefgh".to_owned()),
    ] {
        let session = session_from_codex_thread(
            &json!({"id":"abcdefgh-extra", "cwd":"/work/alpha", "name":name, "preview":preview}),
            &project,
        )
        .unwrap();
        assert_eq!(session.name, expected);
        assert_eq!(session.parent, None);
        assert_eq!(session.model, None);
        assert_eq!(session.started_at, None);
    }
    let short =
        session_from_codex_thread(&json!({"id":"短", "cwd":"/work/alpha"}), &project).unwrap();
    assert_eq!(short.name, "短");
}

#[test]
fn codex_requires_string_id_and_cwd() {
    for thread in [
        json!({}),
        json!({"id":"x"}),
        json!({"cwd":"/work/alpha"}),
        json!({"id":7,"cwd":"/work/alpha"}),
        json!({"id":"x","cwd":null}),
        json!(null),
    ] {
        assert!(session_from_codex_thread(&thread, &project).is_none());
    }
}

#[test]
fn codex_ignores_nonstring_optional_metadata_and_converts_integer_seconds() {
    for seconds in [0_i64, -1, 1700000000] {
        let session = session_from_codex_thread(
            &json!({
                "id":"thread-alpha", "cwd":"/work/alpha",
                "createdAt":seconds, "model":42, "parentThreadId":false
            }),
            &project,
        )
        .unwrap();
        assert_eq!(session.started_at, Some(seconds * 1000));
        assert_eq!(session.model, None);
        assert_eq!(session.parent, None);
        assert_eq!(session.state, SessionState::Finished { failed: false });
        assert_eq!(session.stop, None);
    }
}

#[test]
fn git_common_directory_and_project_names_ignore_trailing_slashes() {
    for common in [
        "/work/alpha/.git",
        "/work/alpha/.git/",
        "/work/alpha/.git///",
    ] {
        assert_eq!(
            project_from_git_common_dir(common),
            Project {
                root: "/work/alpha".into(),
                name: "alpha".into()
            }
        );
    }
    for (root, expected) in [
        ("/work/alpha", "alpha"),
        ("/work/alpha///", "alpha"),
        ("/", "/"),
    ] {
        assert_eq!(project_name(root), expected);
    }
}

#[test]
fn fallback_searches_nearest_ancestor_after_removing_worktree_segments() {
    use std::cell::RefCell;
    for cwd in [
        "/work/alpha/.claude/worktrees/feature/sub",
        "/work/alpha/.agents/worktrees/feature/sub",
        "/work/alpha/sub///",
    ] {
        let calls = RefCell::new(Vec::new());
        let found = fallback_project(cwd, &|path| {
            calls.borrow_mut().push(path.to_owned());
            path == "/work/alpha"
        });
        assert_eq!(
            found,
            Project {
                root: "/work/alpha".into(),
                name: "alpha".into()
            }
        );
        let expected = if cwd.contains("worktrees") {
            vec!["/work/alpha"]
        } else {
            vec!["/work/alpha/sub", "/work/alpha"]
        };
        assert_eq!(*calls.borrow(), expected);
    }
    assert_eq!(
        fallback_project("/work/alpha/sub", &|path| path == "/work/alpha/sub"),
        project_from_git_common_dir("/work/alpha/sub/.git")
    );
    for cwd in [
        "/work/alpha///",
        "/work/alpha/.claude/worktrees/feature",
        "/",
    ] {
        assert_eq!(fallback_project(cwd, &|_| false).root, cwd);
    }
}
