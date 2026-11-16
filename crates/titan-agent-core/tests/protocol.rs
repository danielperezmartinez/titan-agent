use serde_json::json;
use titan_agent_core::protocol::{
    AttachResult, ClientMessage, Event, PROTOCOL_VERSION, Request, ServerMessage, SourceStatus,
    decode_bytes, decode_line, encode_bytes, encode_line, socket_path,
};
use titan_agent_core::session::AgentCli;

#[test]
fn requests_use_method_and_params_with_an_optional_id() {
    let hello = ClientMessage {
        id: Some(1),
        request: Request::Hello {
            protocol: PROTOCOL_VERSION,
            client: "test".into(),
        },
    };
    let value: serde_json::Value = serde_json::to_value(&hello).unwrap();
    assert_eq!(
        value,
        json!({"id": 1, "method": "hello", "params": {"protocol": 1, "client": "test"}})
    );

    let input = ClientMessage {
        id: None,
        request: Request::Input {
            terminal: "t1".into(),
            data: encode_bytes(b"ls\r"),
        },
    };
    let line = encode_line(&input);
    assert!(line.ends_with('\n'));
    assert!(!line.contains("\"id\""));
    assert_eq!(decode_line::<ClientMessage>(&line).unwrap(), input);

    let subscribe: ClientMessage = decode_line(r#"{"id":7,"method":"subscribe"}"#).unwrap();
    assert_eq!(subscribe.request, Request::Subscribe);
    assert_eq!(subscribe.id, Some(7));
}

#[test]
fn unknown_methods_are_rejected() {
    assert!(decode_line::<ClientMessage>(r#"{"id":1,"method":"launch","params":{}}"#).is_err());
}

#[test]
fn server_messages_round_trip() {
    let messages = [
        ServerMessage::result(
            3,
            &AttachResult {
                terminal: "t3".into(),
                created: true,
            },
        ),
        ServerMessage::error(4, "unknown_session", "no existe"),
        ServerMessage::Event(Event::Output {
            terminal: "t3".into(),
            data: encode_bytes(&[0x1b, b'[', b'm']),
        }),
        ServerMessage::Event(Event::Sessions {
            sessions: vec![],
            sources: vec![SourceStatus {
                agent: AgentCli::Codex,
                ok: false,
                detail: Some("sin daemon".into()),
            }],
            terminals: vec![],
            launches: vec![],
            teams: vec![],
            quotas: vec![],
        }),
        ServerMessage::Event(Event::TerminalClosed {
            terminal: "t3".into(),
            session: "cl:x".into(),
            exit_code: Some(0),
        }),
    ];
    for message in messages {
        let line = encode_line(&message);
        assert_eq!(line.matches('\n').count(), 1, "one line per message");
        assert_eq!(decode_line::<ServerMessage>(&line).unwrap(), message);
    }
    let error: ServerMessage =
        decode_line(r#"{"id":4,"error":{"code":"x","message":"y"}}"#).unwrap();
    assert!(matches!(error, ServerMessage::Error { id: Some(4), .. }));
}

#[test]
fn terminal_bytes_survive_base64() {
    let bytes: Vec<u8> = (0..=255).collect();
    assert_eq!(decode_bytes(&encode_bytes(&bytes)).unwrap(), bytes);
    assert!(decode_bytes("%%%").is_none());
}

#[test]
fn socket_path_prefers_the_override_then_the_runtime_folder() {
    assert_eq!(
        socket_path(Some("/x/a.sock"), Some("/run/user/7"), "/tmp", "ana"),
        "/x/a.sock"
    );
    assert_eq!(
        socket_path(Some(""), Some("/run/user/7/"), "/tmp", "ana"),
        "/run/user/7/titan-agent/agentd.sock"
    );
    assert_eq!(
        socket_path(None, None, "/tmp/", "ana"),
        "/tmp/titan-agent-ana/agentd.sock"
    );
}
