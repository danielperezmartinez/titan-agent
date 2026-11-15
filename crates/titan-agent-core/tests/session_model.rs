use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};
use titan_agent_core::session::{
    AgentCli, Project, Session, SessionKey, SessionKind, SessionState,
};

fn assert_wire<T>(value: T, expected: Value)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    assert_eq!(serde_json::to_value(&value).unwrap(), expected);
    assert_eq!(serde_json::from_value::<T>(expected).unwrap(), value);
}

#[test]
fn cli_names_and_order_are_stable() {
    assert_wire(AgentCli::ClaudeCode, json!("claude"));
    assert_wire(AgentCli::Codex, json!("codex"));
    assert!(AgentCli::ClaudeCode < AgentCli::Codex);
    for invalid in [json!("ClaudeCode"), json!("other"), json!(null), json!(1)] {
        assert!(serde_json::from_value::<AgentCli>(invalid).is_err());
    }
}

#[test]
fn every_session_state_has_its_exact_string_representation() {
    for (state, name) in [
        (SessionState::Working, "working"),
        (SessionState::NeedsYou, "needs_you"),
        (SessionState::Waiting, "waiting"),
        (SessionState::Finished { failed: false }, "finished"),
        (SessionState::Finished { failed: true }, "failed"),
    ] {
        assert_wire(state, json!(name));
    }
    for invalid in [
        json!("finished_false"),
        json!("unknown"),
        json!({"failed": true}),
        json!(null),
    ] {
        assert!(serde_json::from_value::<SessionState>(invalid).is_err());
    }
}

#[test]
fn session_keys_preserve_identity_display_hashing_and_field_order() {
    let key = SessionKey::new(AgentCli::ClaudeCode, "sample:α");
    assert_eq!(key.id, "sample:α");
    assert_eq!(key.to_string(), "claude:sample:α");
    assert_eq!(
        SessionKey::new(AgentCli::Codex, String::new()).to_string(),
        "codex:"
    );
    assert_wire(key.clone(), json!({"cli": "claude", "id": "sample:α"}));
    let first = SessionKey::new(AgentCli::ClaudeCode, "z");
    let second = SessionKey::new(AgentCli::Codex, "a");
    let third = SessionKey::new(AgentCli::Codex, "b");
    let ordered: BTreeSet<_> = [third.clone(), second.clone(), first.clone()]
        .into_iter()
        .collect();
    assert_eq!(
        ordered.into_iter().collect::<Vec<_>>(),
        vec![first, second, third]
    );
    let unique: HashSet<_> = [key.clone(), key].into_iter().collect();
    assert_eq!(unique.len(), 1);
}

#[test]
fn kinds_and_projects_support_the_declared_wire_and_value_traits() {
    assert_wire(SessionKind::Interactive, json!("interactive"));
    assert_wire(SessionKind::Background, json!("background"));
    assert!(serde_json::from_value::<SessionKind>(json!("other")).is_err());
    let project = Project {
        name: "sample".into(),
        root: "/work/sample".into(),
    };
    assert_wire(
        project.clone(),
        json!({"name": "sample", "root": "/work/sample"}),
    );
    let later_root = Project {
        root: "/work/z".into(),
        ..project.clone()
    };
    let later_name = Project {
        name: "z".into(),
        root: "/work/a".into(),
    };
    assert!(project < later_root);
    assert!(later_root < later_name);
    let unique: HashSet<_> = [project.clone(), project].into_iter().collect();
    assert_eq!(unique.len(), 1);
    let kinds: HashSet<_> = [SessionKind::Interactive, SessionKind::Background]
        .into_iter()
        .collect();
    assert_eq!(kinds.len(), 2);
}

fn minimal_session_json() -> Value {
    json!({
        "key": {"cli": "codex", "id": "sample-1"},
        "name": "worker", "kind": "background", "state": "needs_you",
        "cwd": "/work/sample/branch", "project": {"name": "sample", "root": "/work/sample"}
    })
}

#[test]
fn session_round_trips_all_populated_fields() {
    let session = Session {
        key: SessionKey::new(AgentCli::Codex, "sample-1"),
        name: "worker".into(),
        kind: SessionKind::Background,
        state: SessionState::NeedsYou,
        cwd: "/work/sample/branch".into(),
        project: Project {
            name: "sample".into(),
            root: "/work/sample".into(),
        },
        model: Some("luna".into()),
        parent: Some(SessionKey::new(AgentCli::Codex, "sample-0")),
        started_at_ms: Some(1_700_000_000_000),
        state_since_ms: Some(-1),
        attach: Some(vec!["codex".into(), "resume".into(), "sample-1".into()]),
    };
    let mut expected = minimal_session_json();
    expected["model"] = json!("luna");
    expected["parent"] = json!({"cli": "codex", "id": "sample-0"});
    expected["started_at_ms"] = json!(1_700_000_000_000_i64);
    expected["state_since_ms"] = json!(-1);
    expected["attach"] = json!(["codex", "resume", "sample-1"]);
    assert_wire(session, expected);
}

#[test]
fn absent_optional_fields_default_to_none_and_serialize_as_null() {
    let mut input = minimal_session_json();
    input["future_field"] = json!({"enabled": true});
    let session: Session = serde_json::from_value(input).unwrap();
    assert_eq!(session.model, None);
    assert_eq!(session.parent, None);
    assert_eq!(session.started_at_ms, None);
    assert_eq!(session.state_since_ms, None);
    assert_eq!(session.attach, None);
    let mut expected = minimal_session_json();
    for field in [
        "model",
        "parent",
        "started_at_ms",
        "state_since_ms",
        "attach",
    ] {
        expected[field] = Value::Null;
    }
    assert_wire(session, expected);
}

#[test]
fn required_session_fields_cannot_be_omitted() {
    for field in ["key", "name", "kind", "state", "cwd", "project"] {
        let mut input = minimal_session_json();
        input.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<Session>(input).is_err(),
            "missing {field}"
        );
    }
}
