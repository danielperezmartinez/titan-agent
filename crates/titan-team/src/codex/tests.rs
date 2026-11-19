use super::*;
use serde_json::json;

#[test]
fn items_become_short_activity_lines() {
    let cwd = std::env::temp_dir().join("w-task");
    let inside = cwd.join("src").join("a.rs");
    let item = json!({"type": "fileChange", "changes": [{"path": inside.to_string_lossy()}, {"path": "b.rs"}]});
    assert_eq!(activity(&item, &cwd).unwrap(), "edita src/a.rs, b.rs");
    let item = json!({"type": "commandExecution", "command": "/usr/bin/bash -lc 'cargo test'"});
    assert_eq!(activity(&item, &cwd).unwrap(), "$ cargo test");
    assert_eq!(
        activity(&json!({"type": "webSearch"}), &cwd).unwrap(),
        "web_search"
    );
    assert_eq!(
        activity(&json!({"type": "mcpToolCall"}), &cwd).unwrap(),
        "mcp_tool_call"
    );
    assert!(activity(&json!({"type": "reasoning"}), &cwd).is_none());
}

#[test]
fn config_values_are_read_as_toml_or_kept_as_text() {
    assert_eq!(toml_value("5"), json!(5));
    assert_eq!(toml_value("\"high\""), json!("high"));
    assert_eq!(toml_value("[\"a\", true]"), json!(["a", true]));
    assert_eq!(toml_value("not toml at all"), json!("not toml at all"));
}

#[test]
fn busy_is_the_active_writer_refusal_in_any_case() {
    assert!(is_busy_message("thread t-1 already has an active writer"));
    assert!(is_busy_message("Thread Already Has An Active Writer"));
    assert!(!is_busy_message("no rollout found"));
}
