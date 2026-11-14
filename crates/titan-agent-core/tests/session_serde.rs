use titan_agent_core::session::{AgentCli, SessionState};

#[test]
fn cli_uses_command_names_on_the_wire() {
    for (cli, name) in [(AgentCli::ClaudeCode, "claude"), (AgentCli::Codex, "codex")] {
        let encoded = serde_json::to_string(&cli).unwrap();
        assert_eq!(encoded, format!("\"{name}\""));
        assert_eq!(serde_json::from_str::<AgentCli>(&encoded).unwrap(), cli);
    }
    for invalid in ["\"Claude Code\"", "\"cl\"", "\"other\"", "null", "1"] {
        assert!(serde_json::from_str::<AgentCli>(invalid).is_err());
    }
}

#[test]
fn states_use_the_specified_strings() {
    for (state, name) in [
        (SessionState::Working, "working"),
        (SessionState::NeedsYou, "needs_you"),
        (SessionState::Waiting, "waiting"),
        (SessionState::Finished { failed: false }, "finished"),
        (SessionState::Finished { failed: true }, "failed"),
    ] {
        let encoded = serde_json::to_string(&state).unwrap();
        assert_eq!(encoded, format!("\"{name}\""));
        assert_eq!(
            serde_json::from_str::<SessionState>(&encoded).unwrap(),
            state
        );
    }
    for invalid in ["\"Working\"", "\"done\"", "\"unknown\"", "null", "{}"] {
        assert!(serde_json::from_str::<SessionState>(invalid).is_err());
    }
}
