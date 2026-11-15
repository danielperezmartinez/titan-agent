use serde_json::json;
use titan_agent_core::discovery::{
    claude_project_dir, claude_state, codex_session, codex_state, last_model_in_transcript,
    link_sessions, parse_claude_agents, team_role,
};
use titan_agent_core::project::{project_name, project_root, team_worktree, tilde};
use titan_agent_core::session::{AgentCli, SessionKind, SessionState, short_model};

const CLAUDE_AGENTS: &str = r#"[
  {"id":"ab12cd34","pid":101,"cwd":"/home/<usuario>/dev/kite/.claude/worktrees/menu","kind":"background",
   "startedAt":1700000000000,"sessionId":"ab12cd34-0000-4000-8000-000000000001",
   "name":"menu: kite","status":"busy","state":"working"},
  {"pid":202,"cwd":"/home/<usuario>/dev/notes","kind":"interactive","startedAt":1700000001000,
   "sessionId":"00000000-0000-4000-8000-000000000002","name":"notes-1a","status":"idle"},
  {"id":"ef56ab78","cwd":"/home/<usuario>/dev/kite","kind":"background","startedAt":1700000002000,
   "sessionId":"ef56ab78-0000-4000-8000-000000000003","state":"blocked"},
  {"cwd":"/nowhere"},
  {"id":"99887766","kind":"background","sessionId":"99887766-0000-4000-8000-000000000004",
   "cwd":"/home/<usuario>/dev/kite","state":"done","futureField":{"x":1}}
]"#;

#[test]
fn claude_agents_json_becomes_sessions() {
    let sessions = parse_claude_agents(CLAUDE_AGENTS).unwrap();
    assert_eq!(sessions.len(), 4, "entries without any id are skipped");

    let bg = &sessions[0];
    assert_eq!(bg.key, "cl:ab12cd34-0000-4000-8000-000000000001");
    assert_eq!(bg.agent, AgentCli::ClaudeCode);
    assert_eq!(bg.cli_id, "ab12cd34");
    assert_eq!(bg.name, "menu: kite");
    assert_eq!(bg.project, "kite");
    assert_eq!(bg.project_root, "/home/<usuario>/dev/kite");
    assert_eq!(bg.kind, SessionKind::Background);
    assert_eq!(bg.state, SessionState::Working);
    assert_eq!(bg.pid, Some(101));
    assert_eq!(bg.started_at_ms, Some(1_700_000_000_000));
    assert_eq!(bg.attach_line().as_deref(), Some("claude attach ab12cd34"));
    assert!(bg.stoppable);

    let interactive = &sessions[1];
    assert_eq!(interactive.kind, SessionKind::Interactive);
    assert_eq!(interactive.state, SessionState::Waiting);
    assert_eq!(
        interactive.attach, None,
        "interactive sessions cannot be attached"
    );
    assert!(!interactive.stoppable);

    let blocked = &sessions[2];
    assert_eq!(blocked.state, SessionState::NeedsYou);
    assert_eq!(
        blocked.name, "claude ef56ab78",
        "a missing name gets a fallback"
    );

    let done = &sessions[3];
    assert_eq!(done.state, SessionState::Finished { failed: false });
    assert!(!done.stoppable);
}

#[test]
fn claude_agents_rejects_what_is_not_an_array() {
    assert!(parse_claude_agents("{}").is_err());
    assert!(parse_claude_agents("not json").is_err());
    assert_eq!(parse_claude_agents("[]").unwrap(), vec![]);
}

#[test]
fn claude_states_follow_status_and_state() {
    use SessionState::*;
    assert_eq!(claude_state(Some("busy"), Some("blocked")), NeedsYou);
    assert_eq!(claude_state(Some("idle"), Some("working")), Waiting);
    assert_eq!(claude_state(Some("busy"), Some("working")), Working);
    assert_eq!(claude_state(None, Some("working")), Working);
    // A background session reports `done` after each turn while it lives.
    assert_eq!(claude_state(Some("idle"), Some("done")), Waiting);
    assert_eq!(claude_state(Some("busy"), Some("done")), Working);
    assert_eq!(claude_state(None, Some("done")), Finished { failed: false });
    assert_eq!(
        claude_state(None, Some("stopped")),
        Finished { failed: false }
    );
    assert_eq!(
        claude_state(None, Some("failed")),
        Finished { failed: true }
    );
    assert_eq!(claude_state(None, None), Waiting);
}

#[test]
fn transcript_tail_gives_the_last_claude_model() {
    let tail = r#"{"type":"assistant","message":{"model":"claude-sonnet-5-5"}}
{"type":"user","model":"<synthetic>"}
{"type":"assistant","message":{"model":"claude-opus-5-5","content":[]}}
{"type":"assistant","message":{"model":"<synthetic>"}}"#;
    assert_eq!(last_model_in_transcript(tail).as_deref(), Some("opus"));
    assert_eq!(last_model_in_transcript("{}"), None);
    assert_eq!(
        claude_project_dir("/home/<usuario>/dev/kite/.claude/worktrees/a"),
        "-home-<usuario>-dev-kite--claude-worktrees-a"
    );
}

#[test]
fn model_names_are_shortened_for_the_pill() {
    assert_eq!(short_model("claude-opus-5-5"), "opus");
    assert_eq!(short_model("claude-haiku-4-5-20251001"), "haiku");
    assert_eq!(short_model("gpt-6-luna"), "luna");
    assert_eq!(short_model("gpt-6.1-sol"), "sol");
    assert_eq!(short_model("o3"), "o3");
}

fn thread(id: &str, cwd: &str, status: serde_json::Value) -> serde_json::Value {
    json!({
        "id": id, "cwd": cwd, "status": status, "source": "exec",
        "preview": "Implementa el renombrado de ficheros en la sesión SFTP con validación",
        "model": "gpt-6-luna", "reasoningEffort": "medium",
        "createdAt": 1_700_000_100, "updatedAt": 1_700_000_200,
        "cliVersion": "0.0.0", "ephemeral": false, "modelProvider": "openai",
        "projectId": null, "sessionId": id, "turns": []
    })
}

#[test]
fn codex_threads_become_sessions() {
    let t = thread(
        "019a-1",
        "/home/<usuario>/dev/kite",
        json!({"type": "active", "activeFlags": []}),
    );
    let session = codex_session(&t, true).unwrap();
    assert_eq!(session.key, "cx:019a-1");
    assert_eq!(session.agent, AgentCli::Codex);
    assert_eq!(session.state, SessionState::Working);
    assert_eq!(session.model.as_deref(), Some("luna"));
    assert_eq!(session.effort.as_deref(), Some("medium"));
    assert_eq!(session.kind, SessionKind::Exec);
    assert_eq!(session.name, "Implementa el renombrado de ficheros en…");
    assert_eq!(session.started_at_ms, Some(1_700_000_100_000));
    assert_eq!(
        session.attach_line().as_deref(),
        Some("codex resume --remote unix:// 019a-1")
    );
    assert!(session.stoppable);

    let mut named = thread("019a-2", "/x", json!({"type": "notLoaded"}));
    named["name"] = json!("Revisa el menú");
    let named = codex_session(&named, false).unwrap();
    assert_eq!(named.name, "Revisa el menú");
    assert_eq!(named.state, SessionState::Finished { failed: false });
    assert_eq!(named.state_since_ms, Some(1_700_000_200_000));
    assert!(!named.stoppable);

    assert!(codex_session(&json!({"cwd": "/x"}), true).is_none());
}

#[test]
fn team_threads_are_named_after_their_role_and_task() {
    let mut t = thread(
        "t9",
        "/r/.agents/worktrees/menu",
        json!({"type": "notLoaded"}),
    );
    t["preview"] = json!("Eres el DISEÑADOR DE TESTS de un equipo. El director…");
    let tests = codex_session(&t, false).unwrap();
    assert_eq!(tests.name, "tests · menu");
    assert_eq!(tests.role.as_deref(), Some("diseñador de tests"));
    t["preview"] = json!("Eres el REVISOR de un equipo.");
    assert_eq!(codex_session(&t, false).unwrap().name, "revisor · menu");
    assert_eq!(
        team_role("Eres el IMPLEMENTADOR de un equipo").as_deref(),
        Some("implementador")
    );
    assert_eq!(team_role("Eres el mejor"), None);
    assert_eq!(team_role("Hola"), None);
    let mut outside = thread("t8", "/r", json!({"type": "idle"}));
    outside["preview"] = json!("Eres el REVISOR de un equipo.");
    assert_eq!(
        codex_session(&outside, true).unwrap().role,
        None,
        "only inside team worktrees"
    );
}

#[test]
fn codex_states_follow_thread_status() {
    use SessionState::*;
    let state = |v: serde_json::Value, loaded| codex_state(Some(&v), loaded);
    assert_eq!(
        state(
            json!({"type":"active","activeFlags":["waitingOnApproval"]}),
            true
        ),
        NeedsYou
    );
    assert_eq!(
        state(json!({"type":"active","activeFlags":[]}), true),
        Working
    );
    assert_eq!(state(json!({"type":"idle"}), true), Waiting);
    assert_eq!(
        state(json!({"type":"systemError"}), true),
        Finished { failed: true }
    );
    assert_eq!(
        state(json!({"type":"notLoaded"}), false),
        Finished { failed: false }
    );
    assert_eq!(state(json!({"type":"brandNew"}), true), Waiting);
    assert_eq!(codex_state(None, false), Finished { failed: false });
}

#[test]
fn team_threads_hang_from_the_director_and_sub_agents_from_their_parent() {
    let claude = parse_claude_agents(
        r#"[{"id":"d1","cwd":"/home/<usuario>/dev/kite/.claude/worktrees/menu","kind":"background",
             "sessionId":"d1-full","name":"menu: kite","status":"busy"}]"#,
    )
    .unwrap();
    let mut sessions = claude;
    let team = thread(
        "t1",
        "/home/<usuario>/dev/kite/.claude/worktrees/menu/.agents/worktrees/renombrar",
        json!({"type":"notLoaded"}),
    );
    sessions.push(codex_session(&team, false).unwrap());
    let mut sub = thread("s1", "/home/<usuario>/dev/kite", json!({"type":"idle"}));
    sub["parentThreadId"] = json!("t1");
    sub["agentRole"] = json!("explorer");
    sessions.push(codex_session(&sub, true).unwrap());
    let mut orphan = thread("o1", "/elsewhere", json!({"type":"idle"}));
    orphan["parentThreadId"] = json!("gone");
    sessions.push(codex_session(&orphan, true).unwrap());
    sessions.push(sessions[0].clone());

    let linked = link_sessions(sessions);
    assert_eq!(linked.len(), 4, "duplicated keys are dropped");
    let get = |key: &str| linked.iter().find(|s| s.key == key).unwrap();
    assert_eq!(get("cx:t1").parent.as_deref(), Some("cl:d1-full"));
    assert_eq!(get("cx:t1").role.as_deref(), Some("equipo · renombrar"));
    assert_eq!(get("cx:s1").parent.as_deref(), Some("cx:t1"));
    assert_eq!(get("cx:s1").role.as_deref(), Some("explorer"));
    assert_eq!(get("cx:o1").parent, None, "unknown parents are dropped");
    assert_eq!(get("cx:o1").role.as_deref(), Some("sesión suelta"));
    assert_eq!(get("cl:d1-full").role.as_deref(), Some("director"));
}

#[test]
fn projects_fold_worktrees_into_their_repository() {
    assert_eq!(
        project_root("/home/<usuario>/dev/kite"),
        "/home/<usuario>/dev/kite"
    );
    assert_eq!(
        project_root("/home/<usuario>/dev/kite/"),
        "/home/<usuario>/dev/kite"
    );
    assert_eq!(
        project_root("/home/<usuario>/dev/kite/.claude/worktrees/menu/.agents/worktrees/x"),
        "/home/<usuario>/dev/kite"
    );
    assert_eq!(project_root("/srv/app/.agents/worktrees/x/src"), "/srv/app");
    assert_eq!(project_root("/"), "/");
    assert_eq!(project_name("/home/<usuario>/dev/kite"), "kite");
    assert_eq!(project_name("/"), "/");
    assert_eq!(
        team_worktree("/r/.claude/worktrees/a/.agents/worktrees/task/sub"),
        Some(("/r/.claude/worktrees/a".to_string(), "task".to_string()))
    );
    assert_eq!(team_worktree("/r/.agents/worktrees/"), None);
    assert_eq!(
        tilde("/home/<usuario>/dev", Some("/home/<usuario>")),
        "~/dev"
    );
    assert_eq!(
        tilde("/home/<usuario>bel", Some("/home/<usuario>")),
        "/home/<usuario>bel"
    );
    assert_eq!(tilde("/home/<usuario>", Some("/home/<usuario>/")), "~");
}
