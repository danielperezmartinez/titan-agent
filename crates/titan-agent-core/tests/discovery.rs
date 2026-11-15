use serde_json::{Value, json};
use titan_agent_core::discovery::{
    ClaudeAgent, CodexThread, DiscoveryError, claude_model_from_transcript, claude_session,
    claude_state, codex_session, codex_state, display_path, parse_claude_agents,
    parse_codex_thread, project_for, short_model,
};
use titan_agent_core::session::{
    AgentCli, Project, Session, SessionKey, SessionKind, SessionState,
};

fn project() -> Project {
    Project {
        name: "sample".into(),
        root: "/work/sample".into(),
    }
}

fn claude_agent() -> ClaudeAgent {
    ClaudeAgent {
        id: Some("agent-42".into()),
        pid: Some(721),
        cwd: "/work/sample/feature".into(),
        kind: SessionKind::Background,
        started_at_ms: Some(1_790_000_000_123),
        session_id: Some("transcript-42".into()),
        name: "Sample feature".into(),
        status: Some("busy".into()),
        state: Some("blocked".into()),
    }
}

fn codex_thread() -> CodexThread {
    CodexThread {
        id: "thread-42-example".into(),
        name: Some("Sample review".into()),
        preview: Some("Preview text".into()),
        cwd: "/work/sample/feature".into(),
        created_at_s: Some(1_790_000_000),
        state: Some(SessionState::NeedsYou),
        model: Some("gpt-6.1-sol".into()),
        parent_thread_id: Some("parent-41".into()),
        source: Some("appServer".into()),
    }
}

#[test]
fn claude_array_preserves_order_and_all_declared_fields() {
    let parsed = parse_claude_agents(
        &json!([
            {"id":"agent-42", "pid":721, "cwd":"/work/sample/feature",
             "kind":"background", "startedAt":1_790_000_000_123_i64,
             "sessionId":"transcript-42", "name":"Sample feature",
             "status":"busy", "state":"blocked", "future":{"enabled":true}},
            {"cwd":"/work/control", "name":"Control", "kind":"interactive"}
        ])
        .to_string(),
    )
    .unwrap();
    assert_eq!(parsed[0], claude_agent());
    assert_eq!(
        parsed[1],
        ClaudeAgent {
            id: None,
            pid: None,
            cwd: "/work/control".into(),
            kind: SessionKind::Interactive,
            started_at_ms: None,
            session_id: None,
            name: "Control".into(),
            status: None,
            state: None,
        }
    );
    assert_eq!(parsed.len(), 2);
    assert!(parse_claude_agents("[]").unwrap().is_empty());
}

#[test]
fn claude_parser_distinguishes_invalid_json_from_non_arrays() {
    for input in ["", "[", "[{]", "[] trailing"] {
        assert!(matches!(
            parse_claude_agents(input),
            Err(DiscoveryError::Json(_))
        ));
    }
    for input in ["null", "{}", "42", "true", "\"text\""] {
        assert_eq!(parse_claude_agents(input), Err(DiscoveryError::NotAnArray));
    }
    fn assert_error<T: std::error::Error>() {}
    assert_error::<DiscoveryError>();
    for error in [
        DiscoveryError::NotAnArray,
        DiscoveryError::Json("bad JSON".into()),
    ] {
        let _ = error.to_string();
        assert_eq!(error.clone(), error);
    }
}

#[test]
fn claude_parser_skips_only_invalid_entries() {
    let valid = json!({"cwd":"/work/sample", "name":"Sample", "kind":"interactive"});
    let mut entries = vec![
        Value::Null,
        json!(false),
        json!(7),
        json!("text"),
        json!([]),
    ];
    for field in ["cwd", "name", "kind"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        entries.push(missing);
        for wrong in [Value::Null, json!(3), json!(true), json!([]), json!({})] {
            let mut entry = valid.clone();
            entry[field] = wrong;
            entries.push(entry);
        }
    }
    for kind in ["unknown", "Interactive", ""] {
        let mut entry = valid.clone();
        entry["kind"] = json!(kind);
        entries.push(entry);
    }
    entries.insert(0, valid.clone());
    entries.push(valid);
    let parsed = parse_claude_agents(&Value::Array(entries).to_string()).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0], parsed[1]);
}

#[test]
fn claude_optional_fields_ignore_null_and_wrong_types_independently() {
    for field in ["id", "pid", "startedAt", "sessionId", "status", "state"] {
        let valid = json!({"cwd":"/work/sample", "name":"Sample", "kind":"background"});
        let baseline = parse_claude_agents(&json!([valid.clone()]).to_string()).unwrap();
        let invalid = match field {
            "pid" => vec![
                Value::Null,
                json!("721"),
                json!(-1),
                json!(4_294_967_296_u64),
                json!(1.5),
                json!(true),
                json!({}),
                json!([]),
            ],
            "startedAt" => vec![
                Value::Null,
                json!("123"),
                json!(1.5),
                json!(u64::MAX),
                json!(true),
                json!({}),
                json!([]),
            ],
            _ => vec![Value::Null, json!(12), json!(false), json!({}), json!([])],
        };
        for value in invalid {
            let mut entry = valid.clone();
            entry[field] = value;
            assert_eq!(
                parse_claude_agents(&json!([entry]).to_string()).unwrap(),
                baseline,
                "{field}"
            );
        }
    }
    for (pid, timestamp) in [(0, i64::MIN), (u32::MAX, i64::MAX)] {
        let parsed = parse_claude_agents(
            &json!([{
                "cwd":"/work/sample", "name":"Sample", "kind":"interactive",
                "pid":pid, "startedAt":timestamp
            }])
            .to_string(),
        )
        .unwrap();
        assert_eq!(parsed[0].pid, Some(pid));
        assert_eq!(parsed[0].started_at_ms, Some(timestamp));
    }
}

#[test]
fn claude_state_obeys_the_full_precedence_table() {
    for status in [None, Some("busy"), Some("idle"), Some("unknown")] {
        for (state, expected) in [
            ("blocked", SessionState::NeedsYou),
            ("done", SessionState::Finished { failed: false }),
            ("stopped", SessionState::Finished { failed: false }),
            ("failed", SessionState::Finished { failed: true }),
            ("error", SessionState::Finished { failed: true }),
        ] {
            assert_eq!(claude_state(status, Some(state)), expected);
        }
    }
    for state in [None, Some("working"), Some("unknown"), Some("")] {
        assert_eq!(claude_state(Some("busy"), state), SessionState::Working);
        assert_eq!(claude_state(Some("idle"), state), SessionState::Waiting);
    }
    for status in [None, Some("unknown"), Some("")] {
        assert_eq!(claude_state(status, Some("working")), SessionState::Working);
        for state in [None, Some("unknown"), Some("")] {
            assert_eq!(claude_state(status, state), SessionState::Waiting);
        }
    }
}

#[test]
fn claude_session_maps_all_fields_and_prefers_short_id_over_pid() {
    assert_eq!(
        claude_session(&claude_agent(), project(), Some("opus".into())),
        Some(Session {
            key: SessionKey::new(AgentCli::ClaudeCode, "agent-42"),
            name: "Sample feature".into(),
            kind: SessionKind::Background,
            state: SessionState::NeedsYou,
            cwd: "/work/sample/feature".into(),
            project: project(),
            model: Some("opus".into()),
            parent: None,
            started_at_ms: Some(1_790_000_000_123),
            state_since_ms: None,
            attach: Some(vec!["claude".into(), "attach".into(), "agent-42".into()]),
        })
    );
}

#[test]
fn claude_session_requires_identity_and_only_attaches_background_ids() {
    for kind in [SessionKind::Interactive, SessionKind::Background] {
        for (id, pid, expected_id) in [
            (Some("short"), Some(721), Some("short")),
            (Some("short"), None, Some("short")),
            (None, Some(721), Some("pid-721")),
            (None, Some(0), Some("pid-0")),
            (None, None, None),
        ] {
            let agent = ClaudeAgent {
                id: id.map(str::to_owned),
                pid,
                kind: kind.clone(),
                status: None,
                state: None,
                started_at_ms: None,
                ..claude_agent()
            };
            let result = claude_session(&agent, project(), None);
            let Some(expected_id) = expected_id else {
                assert!(result.is_none());
                continue;
            };
            let session = result.unwrap();
            assert_eq!(
                session.key,
                SessionKey::new(AgentCli::ClaudeCode, expected_id)
            );
            assert_eq!(session.kind, kind);
            assert_eq!(session.state, SessionState::Waiting);
            assert_eq!(session.model, None);
            assert_eq!(session.started_at_ms, None);
            assert_eq!(session.parent, None);
            assert_eq!(session.state_since_ms, None);
            let expected_attach = if kind == SessionKind::Background {
                id.map(|id| vec!["claude".into(), "attach".into(), id.into()])
            } else {
                None
            };
            assert_eq!(session.attach, expected_attach);
        }
    }
}

#[test]
fn transcript_uses_latest_eligible_assistant_model_and_skips_cut_lines() {
    let tail = concat!(
        "cut first line\n",
        "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-5-5\"}}\n",
        "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-3-5-sonnet-20241022[1m]\"}}\n",
        "{\"type\":\"assistant\",\"message\":{\"model\":\"<synthetic>\"}}\n",
        "{\"type\":\"user\",\"message\":{\"model\":\"ignored\"}}\n",
        "{\"type\":\"assistant\",\"message\":{\"model\":42}}\n",
        "{\"type\":\"assistant\",\"message\":null}\n",
        "{\"type\":\"assistant\"}\n",
        "[]\nnull\n42\ninvalid last line\n\n"
    );
    assert_eq!(claude_model_from_transcript(tail), Some("sonnet".into()));
    assert_eq!(
        claude_model_from_transcript("{\"type\":\"assistant\",\"message\":{\"model\":\"o3\"}}"),
        Some("o3".into())
    );
}

#[test]
fn transcript_without_eligible_model_returns_none() {
    for tail in [
        "",
        "\n \n",
        "cut JSON",
        "[]\nnull\n{}",
        r#"{"type":"assistant","message":{"model":"<synthetic>"}}"#,
        r#"{"type":"user","message":{"model":"claude-opus-5-5"}}"#,
    ] {
        assert_eq!(claude_model_from_transcript(tail), None);
    }
}

#[test]
fn codex_parser_maps_all_fields_and_ignores_unknown_fields() {
    let value = json!({
        "id":"thread-42-example", "name":"Sample review", "preview":"Preview text",
        "cwd":"/work/sample/feature", "createdAt":1_790_000_000,
        "status":{"type":"active", "activeFlags":["waitingOnApproval"]},
        "model":"gpt-6.1-sol", "parentThreadId":"parent-41", "source":"appServer",
        "future": {"enabled":true}
    });
    assert_eq!(parse_codex_thread(&value), Some(codex_thread()));
    assert_eq!(
        parse_codex_thread(&json!({"id":"t", "cwd":"/work/sample"})),
        Some(CodexThread {
            id: "t".into(),
            name: None,
            preview: None,
            cwd: "/work/sample".into(),
            created_at_s: None,
            state: Some(SessionState::Waiting),
            model: None,
            parent_thread_id: None,
            source: None,
        })
    );
}

#[test]
fn codex_parser_rejects_non_objects_and_missing_or_non_string_identity_fields() {
    for value in [
        Value::Null,
        json!(false),
        json!(4),
        json!("text"),
        json!([]),
    ] {
        assert!(parse_codex_thread(&value).is_none());
    }
    for field in ["id", "cwd"] {
        let mut missing = json!({"id":"thread", "cwd":"/work/sample"});
        missing.as_object_mut().unwrap().remove(field);
        assert!(parse_codex_thread(&missing).is_none());
        for wrong in [Value::Null, json!(3), json!(true), json!([]), json!({})] {
            let mut value = json!({"id":"thread", "cwd":"/work/sample"});
            value[field] = wrong;
            assert!(parse_codex_thread(&value).is_none());
        }
    }
}

#[test]
fn codex_optional_fields_ignore_null_and_wrong_types() {
    let base = json!({"id":"thread", "cwd":"/work/sample"});
    let baseline = parse_codex_thread(&base);
    for field in [
        "name",
        "preview",
        "createdAt",
        "model",
        "parentThreadId",
        "source",
    ] {
        let invalid = if field == "createdAt" {
            vec![
                Value::Null,
                json!("123"),
                json!(1.5),
                json!(u64::MAX),
                json!(true),
                json!([]),
                json!({}),
            ]
        } else {
            vec![Value::Null, json!(3), json!(false), json!([]), json!({})]
        };
        for wrong in invalid {
            let mut value = base.clone();
            value[field] = wrong;
            assert_eq!(parse_codex_thread(&value), baseline, "{field}");
        }
    }
    for timestamp in [i64::MIN, 0, i64::MAX] {
        let mut value = base.clone();
        value["createdAt"] = json!(timestamp);
        assert_eq!(
            parse_codex_thread(&value).unwrap().created_at_s,
            Some(timestamp)
        );
    }
}

#[test]
fn codex_state_maps_all_status_types_and_approval_flags() {
    for (status, expected) in [
        (json!({"type":"idle"}), Some(SessionState::Waiting)),
        (
            json!({"type":"systemError"}),
            Some(SessionState::Finished { failed: true }),
        ),
        (json!({"type":"notLoaded"}), None),
        (json!({"type":"active"}), Some(SessionState::Working)),
        (
            json!({"type":"active", "activeFlags":[]}),
            Some(SessionState::Working),
        ),
        (
            json!({"type":"active", "activeFlags":["other"]}),
            Some(SessionState::Working),
        ),
    ] {
        assert_eq!(codex_state(&status), expected);
        let thread =
            parse_codex_thread(&json!({"id":"t", "cwd":"/work/sample", "status":status})).unwrap();
        assert_eq!(thread.state, expected);
    }
    for flags in [
        json!(["waitingOnApproval"]),
        json!(["waitingOnUserInput"]),
        json!(["other", "waitingOnApproval", "waitingOnUserInput"]),
    ] {
        assert_eq!(
            codex_state(&json!({"type":"active", "activeFlags":flags})),
            Some(SessionState::NeedsYou)
        );
    }
    assert_eq!(
        codex_state(&json!({"type":"idle", "activeFlags":["waitingOnApproval"]})),
        Some(SessionState::Waiting)
    );
}

#[test]
fn codex_missing_or_unknown_status_defaults_to_waiting() {
    for status in [
        Value::Null,
        json!({}),
        json!({"type":"unknown"}),
        json!({"type":null}),
        json!({"type":3}),
    ] {
        assert_eq!(codex_state(&status), Some(SessionState::Waiting));
    }
}

#[test]
fn codex_session_maps_all_fields_and_units() {
    assert_eq!(
        codex_session(&codex_thread(), project()),
        Some(Session {
            key: SessionKey::new(AgentCli::Codex, "thread-42-example"),
            name: "Sample review".into(),
            kind: SessionKind::Background,
            state: SessionState::NeedsYou,
            cwd: "/work/sample/feature".into(),
            project: project(),
            model: Some("sol".into()),
            parent: Some(SessionKey::new(AgentCli::Codex, "parent-41")),
            started_at_ms: Some(1_790_000_000_000),
            state_since_ms: None,
            attach: Some(vec![
                "codex".into(),
                "resume".into(),
                "thread-42-example".into()
            ]),
        })
    );
    let unloaded = CodexThread {
        state: None,
        ..codex_thread()
    };
    assert!(codex_session(&unloaded, project()).is_none());
}

#[test]
fn codex_session_source_controls_kind_and_absent_metadata_stays_absent() {
    for (source, kind) in [
        (Some("exec"), SessionKind::Background),
        (Some("appServer"), SessionKind::Background),
        (Some("vscode"), SessionKind::Interactive),
        (Some("unknown"), SessionKind::Interactive),
        (Some(""), SessionKind::Interactive),
        (None, SessionKind::Interactive),
    ] {
        let thread = CodexThread {
            source: source.map(str::to_owned),
            model: None,
            parent_thread_id: None,
            created_at_s: None,
            ..codex_thread()
        };
        let session = codex_session(&thread, project()).unwrap();
        assert_eq!(session.kind, kind);
        assert_eq!(session.model, None);
        assert_eq!(session.parent, None);
        assert_eq!(session.started_at_ms, None);
        assert_eq!(session.state_since_ms, None);
    }
    for seconds in [-123, 0, 123] {
        let thread = CodexThread {
            created_at_s: Some(seconds),
            ..codex_thread()
        };
        assert_eq!(
            codex_session(&thread, project()).unwrap().started_at_ms,
            Some(seconds * 1000)
        );
    }
}

#[test]
fn codex_explicit_name_is_trimmed_and_takes_precedence_over_preview() {
    let thread = CodexThread {
        name: Some(" \tSample review \n ".into()),
        preview: Some("Different preview".into()),
        ..codex_thread()
    };
    assert_eq!(
        codex_session(&thread, project()).unwrap().name,
        "Sample review"
    );
}

#[test]
fn codex_names_fall_back_to_trimmed_first_preview_line_then_short_id() {
    for name in [None, Some(""), Some(" \t\n ")] {
        for (preview, expected) in [
            (
                Some("  First preview line  \nSecond line"),
                "First preview line",
            ),
            (Some("\nSecond line"), "codex thread-4"),
            (Some(" \t \nSecond line"), "codex thread-4"),
            (Some(" \t "), "codex thread-4"),
            (Some(""), "codex thread-4"),
            (None, "codex thread-4"),
        ] {
            let thread = CodexThread {
                name: name.map(str::to_owned),
                preview: preview.map(str::to_owned),
                ..codex_thread()
            };
            assert_eq!(codex_session(&thread, project()).unwrap().name, expected);
        }
    }
    for id in ["tiny", "", "αβγδεζηθικ"] {
        let thread = CodexThread {
            id: id.into(),
            name: None,
            preview: None,
            ..codex_thread()
        };
        assert_eq!(
            codex_session(&thread, project()).unwrap().name,
            format!("codex {}", id.chars().take(8).collect::<String>())
        );
    }
}

#[test]
fn codex_preview_limit_counts_characters_and_adds_ellipsis_only_when_cut() {
    for character in ['a', 'é', '🦀'] {
        for length in [47, 48, 49] {
            let text = character.to_string().repeat(length);
            let thread = CodexThread {
                name: None,
                preview: Some(format!("  {text}  \nIgnored")),
                ..codex_thread()
            };
            let expected = if length > 48 {
                format!("{}…", character.to_string().repeat(48))
            } else {
                text
            };
            assert_eq!(codex_session(&thread, project()).unwrap().name, expected);
        }
    }
    let name = "n".repeat(60);
    let thread = CodexThread {
        name: Some(name.clone()),
        ..codex_thread()
    };
    assert_eq!(codex_session(&thread, project()).unwrap().name, name);
}

#[test]
fn model_shortening_follows_prefix_segments_and_trailing_context_suffix() {
    for (input, expected) in [
        ("claude-opus-5-5[1m]", "opus"),
        ("claude-opus-5-5", "opus"),
        ("claude-3-5-sonnet-20241022", "sonnet"),
        ("claude-3-5-sonnet-20241022[200k]", "sonnet"),
        ("gpt-6-luna", "luna"),
        ("gpt-6.1-sol", "sol"),
        ("gpt-5.4", "5.4"),
        ("gpt-123-4.5", "123-4.5"),
        ("gpt-sol-luna", "sol"),
        ("gpt-v2-luna", "luna"),
        ("gpt-é-luna", "luna"),
        ("claude-123", "123"),
        ("gpt-SOL-6", "SOL"),
        ("gpt-", ""),
        ("claude-", ""),
        ("o3", "o3"),
        ("opus", "opus"),
        ("", ""),
        ("custom-model", "custom-model"),
        ("o3[1m]", "o3"),
    ] {
        assert_eq!(short_model(input), expected, "{input}");
    }
}

#[test]
fn worktree_markers_override_repository_detection_and_choose_first_marker() {
    for cwd in [
        "/work/sample/.claude/worktrees/feature/subdir",
        "/work/sample/.agents/worktrees/feature",
        "/work/sample/.worktrees/feature/",
        "/work/sample/.agents/worktrees/feature/.claude/worktrees/nested",
        "/work/sample/.worktrees/feature/.agents/worktrees/nested",
        "/work/sample/.claude/worktrees/feature/.worktrees/nested",
    ] {
        assert_eq!(project_for(cwd, |_| true), project());
        assert_eq!(project_for(cwd, |_| false), project());
    }
}

#[test]
fn project_uses_nearest_repository_including_cwd_and_filesystem_root() {
    assert_eq!(
        project_for("/work/sample", |path| path == "/work/sample"),
        project()
    );
    assert_eq!(
        project_for("/work/sample/src/deep/", |path| matches!(
            path,
            "/work" | "/work/sample"
        )),
        project()
    );
    assert_eq!(
        project_for("/work/sample", |path| path == "/"),
        Project {
            name: "/".into(),
            root: "/".into()
        }
    );
}

#[test]
fn project_falls_back_to_cwd_and_ignores_trailing_slashes() {
    for cwd in ["/work/sample", "/work/sample/", "/work/sample///"] {
        assert_eq!(project_for(cwd, |_| false), project());
    }
    assert_eq!(
        project_for("/", |_| false),
        Project {
            name: "/".into(),
            root: "/".into()
        }
    );
    for cwd in [
        "/work/sample/.worktrees-other/branch",
        "/work/sample/.claude/worktrees-other/branch",
        "/work/sample/.agents/worktrees-other/branch",
    ] {
        assert_eq!(
            project_for(cwd, |_| false),
            Project {
                name: "branch".into(),
                root: cwd.into()
            }
        );
    }
}

#[test]
fn display_path_shortens_only_home_and_its_descendants() {
    for (path, home, expected) in [
        ("/srv/people/sample", Some("/srv/people/sample"), "~"),
        (
            "/srv/people/sample/project",
            Some("/srv/people/sample"),
            "~/project",
        ),
        (
            "/srv/people/sample/project/deep",
            Some("/srv/people/sample"),
            "~/project/deep",
        ),
        (
            "/srv/people/sample-other/project",
            Some("/srv/people/sample"),
            "/srv/people/sample-other/project",
        ),
        (
            "/srv/people/sample",
            Some("/srv/people/sam"),
            "/srv/people/sample",
        ),
        (
            "/elsewhere/project",
            Some("/srv/people/sample"),
            "/elsewhere/project",
        ),
        (
            "/srv/people/sample/project",
            None,
            "/srv/people/sample/project",
        ),
        (
            "/srv/people/sample/project",
            Some(""),
            "/srv/people/sample/project",
        ),
        ("", Some("/srv/people/sample"), ""),
    ] {
        assert_eq!(display_path(path, home), expected);
    }
}

#[test]
fn parsers_accept_empty_required_strings_because_only_types_are_required() {
    let agents = parse_claude_agents(
        r#"[{"cwd":"","name":"","kind":"interactive","id":"","sessionId":"","status":"","state":""}]"#,
    )
    .unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].cwd, "");
    assert_eq!(agents[0].name, "");
    assert_eq!(agents[0].id.as_deref(), Some(""));
    assert_eq!(agents[0].session_id.as_deref(), Some(""));
    assert_eq!(agents[0].status.as_deref(), Some(""));
    assert_eq!(agents[0].state.as_deref(), Some(""));
    let thread = parse_codex_thread(&json!({"id":"", "cwd":""})).unwrap();
    assert_eq!(thread.id, "");
    assert_eq!(thread.cwd, "");
}

#[test]
fn codex_session_preserves_every_loaded_state() {
    for state in [
        SessionState::Working,
        SessionState::NeedsYou,
        SessionState::Waiting,
        SessionState::Finished { failed: false },
        SessionState::Finished { failed: true },
    ] {
        let thread = CodexThread {
            state: Some(state.clone()),
            ..codex_thread()
        };
        assert_eq!(codex_session(&thread, project()).unwrap().state, state);
    }
}
