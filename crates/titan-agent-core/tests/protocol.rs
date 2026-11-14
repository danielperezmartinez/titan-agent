use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Cursor, Read};
use titan_agent_core::protocol::{
    decode_line, encode_line, error_code, ClientRequest, DaemonNotification, DecodeError,
    HelloResult, LineError, LineReader, Message, PeerInfo, Project, RpcError, Session,
    SessionsResult, StopAction, MAX_LINE_BYTES, PROTOCOL_VERSION,
};
use titan_agent_core::session::{AgentCli, SessionState};

fn peer() -> PeerInfo {
    PeerInfo {
        name: "example-client".into(),
        version: "0.1-test".into(),
    }
}
fn session() -> Session {
    Session {
        id: "claude:alpha".into(),
        cli: AgentCli::ClaudeCode,
        name: "Example\ntask".into(),
        cwd: "/work/alpha".into(),
        project: Project {
            root: "/work/alpha".into(),
            name: "Alpha".into(),
        },
        state: SessionState::Working,
        parent: None,
        pid: None,
        started_at: None,
        model: None,
        attach: None,
        stop: None,
    }
}
fn assert_wire(message: Message, expected: Value) {
    let encoded = encode_line(&message);
    assert!(encoded.ends_with('\n'));
    assert_eq!(encoded.bytes().filter(|&b| b == b'\n').count(), 1);
    assert_eq!(serde_json::from_str::<Value>(&encoded).unwrap(), expected);
    for line in [
        encoded.clone(),
        encoded.trim_end_matches('\n').to_owned(),
        format!("{}\r\n", encoded.trim_end_matches('\n')),
    ] {
        assert_eq!(decode_line(&line), Ok(message.clone()));
    }
}

#[test]
fn constants_match_the_protocol_contract() {
    assert_eq!(PROTOCOL_VERSION, 1);
    assert_eq!(MAX_LINE_BYTES, 4 * 1024 * 1024);
    assert_eq!(
        [
            error_code::UNSUPPORTED_PROTOCOL,
            error_code::HELLO_REQUIRED,
            error_code::UNKNOWN_SESSION,
            error_code::NOT_AVAILABLE,
            error_code::INVALID_PARAMS,
            error_code::UNKNOWN_METHOD,
            error_code::INTERNAL
        ],
        [1, 2, 3, 4, 5, 6, 7]
    );
}

#[test]
fn every_request_has_the_expected_wire_shape_and_round_trips() {
    for (request, method, params) in [
        (
            ClientRequest::Hello {
                protocol: 1,
                client: peer(),
            },
            "hello",
            json!({"protocol":1,"client":{"name":"example-client","version":"0.1-test"}}),
        ),
        (ClientRequest::SessionsList, "sessions/list", json!({})),
        (
            ClientRequest::SessionsSubscribe,
            "sessions/subscribe",
            json!({}),
        ),
        (
            ClientRequest::TerminalOpen {
                session: "claude:alpha".into(),
                cols: 80,
                rows: 24,
            },
            "terminal/open",
            json!({"session":"claude:alpha","cols":80,"rows":24}),
        ),
        (
            ClientRequest::TerminalInput {
                session: "claude:alpha".into(),
                data: vec![0, 255, 128, 10],
            },
            "terminal/input",
            json!({"session":"claude:alpha","data":"AP+ACg=="}),
        ),
        (
            ClientRequest::TerminalResize {
                session: "claude:alpha".into(),
                cols: u16::MAX,
                rows: 0,
            },
            "terminal/resize",
            json!({"session":"claude:alpha","cols":65535,"rows":0}),
        ),
        (
            ClientRequest::TerminalClose {
                session: "claude:alpha".into(),
            },
            "terminal/close",
            json!({"session":"claude:alpha"}),
        ),
        (
            ClientRequest::SessionStop {
                session: "claude:alpha".into(),
            },
            "session/stop",
            json!({"session":"claude:alpha"}),
        ),
    ] {
        assert_wire(
            Message::Request { id: 1, request },
            json!({"id":1,"method":method,"params":params}),
        );
    }
    for (method, request) in [
        ("sessions/list", ClientRequest::SessionsList),
        ("sessions/subscribe", ClientRequest::SessionsSubscribe),
    ] {
        assert_eq!(
            decode_line(&json!({"id":0,"method":method}).to_string()),
            Ok(Message::Request { id: 0, request })
        );
    }
}

#[test]
fn notifications_and_responses_have_expected_wire_shapes() {
    let s = session();
    assert_wire(
        Message::Notification(DaemonNotification::SessionsChanged {
            sessions: vec![s.clone()],
        }),
        json!({"method":"sessions/changed","params":{"sessions":[s]}}),
    );
    for data in [vec![], vec![104, 105], vec![0, 255, 128, 10]] {
        let encoded = match data.len() {
            0 => "",
            2 => "aGk=",
            _ => "AP+ACg==",
        };
        assert_wire(
            Message::Notification(DaemonNotification::TerminalOutput {
                session: "codex:beta".into(),
                data,
            }),
            json!({"method":"terminal/output","params":{"session":"codex:beta","data":encoded}}),
        );
    }
    assert_wire(
        Message::Request {
            id: u64::MAX,
            request: ClientRequest::TerminalInput {
                session: "codex:beta".into(),
                data: vec![],
            },
        },
        json!({"id":u64::MAX,"method":"terminal/input","params":{"session":"codex:beta","data":""}}),
    );
    for code in [None, Some(0), Some(-1), Some(i32::MAX)] {
        assert_wire(
            Message::Notification(DaemonNotification::TerminalExited {
                session: "codex:beta".into(),
                code,
            }),
            json!({"method":"terminal/exited","params":{"session":"codex:beta","code":code}}),
        );
    }
    for result in [
        Value::Null,
        json!({"sessions":[]}),
        json!([1, "x"]),
        json!(false),
    ] {
        assert_wire(
            Message::Response {
                id: 7,
                result: Ok(result.clone()),
            },
            json!({"id":7,"result":result}),
        );
    }
    assert_wire(
        Message::Response {
            id: 8,
            result: Err(RpcError {
                code: 3,
                message: "Unknown example session\nretry".into(),
            }),
        },
        json!({"id":8,"error":{"code":3,"message":"Unknown example session\nretry"}}),
    );
}

#[test]
fn session_options_are_explicit_null_but_may_be_missing_on_input() {
    let minimal = json!({"id":"claude:alpha","cli":"claude","name":"Example\ntask","cwd":"/work/alpha","project":{"root":"/work/alpha","name":"Alpha"},"state":"working"});
    let mut expected = minimal.clone();
    for field in ["parent", "pid", "started_at", "model", "attach", "stop"] {
        expected[field] = Value::Null;
    }
    assert_eq!(serde_json::to_value(session()).unwrap(), expected);
    let mut extra = minimal;
    extra["future_field"] = json!({"anything":true});
    assert_eq!(serde_json::from_value::<Session>(extra).unwrap(), session());
    let mut full = session();
    full.parent = Some("claude:parent".into());
    full.pid = Some(4242);
    full.started_at = Some(1700000000123);
    full.model = Some("invented-model".into());
    full.attach = Some(vec!["claude".into(), "attach".into(), "alpha".into()]);
    full.stop = Some(StopAction::Stop);
    let encoded = serde_json::to_value(&full).unwrap();
    assert_eq!(encoded["started_at"], 1700000000123_i64);
    assert_eq!(encoded["stop"], "stop");
    assert_eq!(serde_json::from_value::<Session>(encoded).unwrap(), full);
    assert_eq!(
        serde_json::to_value(StopAction::Interrupt).unwrap(),
        "interrupt"
    );
    assert_eq!(
        serde_json::from_value::<StopAction>(json!("interrupt")).unwrap(),
        StopAction::Interrupt
    );
    assert!(serde_json::from_value::<StopAction>(json!("unknown")).is_err());
}

#[test]
fn response_payload_structs_use_documented_fields() {
    let hello = HelloResult {
        protocol: 1,
        daemon: peer(),
    };
    let value = json!({"protocol":1,"daemon":{"name":"example-client","version":"0.1-test"}});
    assert_eq!(serde_json::to_value(&hello).unwrap(), value);
    assert_eq!(serde_json::from_value::<HelloResult>(value).unwrap(), hello);
    let sessions = SessionsResult {
        sessions: vec![session()],
    };
    let value = json!({"sessions":[session()]});
    assert_eq!(serde_json::to_value(&sessions).unwrap(), value);
    assert_eq!(
        serde_json::from_value::<SessionsResult>(value).unwrap(),
        sessions
    );
}

#[test]
fn malformed_json_and_non_message_shapes_are_invalid_json() {
    for line in [
        "",
        "{",
        "[]",
        "null",
        "42",
        "\"text\"",
        "{}",
        "{\"id\":1}",
        "{\"result\":{}}",
        "{\"error\":{}}",
        "{\"id\":-1,\"result\":null}",
        "{\"id\":\"1\",\"result\":null}",
        "{\"method\":3}",
    ] {
        assert!(
            matches!(decode_line(line), Err(DecodeError::InvalidJson(_))),
            "{line}"
        );
    }
}

#[test]
fn unknown_methods_preserve_request_id_or_notification_absence() {
    for id in [None, Some(19)] {
        let mut value = json!({"method":"example/unknown","params":{}});
        if let Some(id) = id {
            value["id"] = json!(id);
        }
        assert_eq!(
            decode_line(&value.to_string()),
            Err(DecodeError::UnknownMethod {
                id,
                method: "example/unknown".into()
            })
        );
    }
}

#[test]
fn known_methods_reject_missing_and_wrong_parameters() {
    for method in [
        "hello",
        "terminal/open",
        "terminal/input",
        "terminal/resize",
        "terminal/close",
        "session/stop",
    ] {
        for params in [None, Some(json!({})), Some(json!([])), Some(json!(null))] {
            let mut value = json!({"id":9,"method":method});
            if let Some(params) = params {
                value["params"] = params;
            }
            assert!(
                matches!(
                    decode_line(&value.to_string()),
                    Err(DecodeError::InvalidParams { id: Some(9), .. })
                ),
                "{value}"
            );
        }
    }
    for (method, params) in [
        (
            "terminal/open",
            json!({"session":"claude:alpha","cols":65536,"rows":24}),
        ),
        (
            "terminal/resize",
            json!({"session":"claude:alpha","cols":80,"rows":-1}),
        ),
        ("terminal/open", json!({"session":7,"cols":80,"rows":24})),
        (
            "hello",
            json!({"protocol":"1","client":{"name":"example","version":"1"}}),
        ),
    ] {
        assert!(matches!(
            decode_line(&json!({"id":9,"method":method,"params":params}).to_string()),
            Err(DecodeError::InvalidParams { id: Some(9), .. })
        ));
    }
    for method in ["sessions/changed", "terminal/output", "terminal/exited"] {
        assert!(matches!(
            decode_line(&json!({"method":method}).to_string()),
            Err(DecodeError::InvalidParams { id: None, .. })
        ));
    }
    for data in ["!", "aGk", "____", "aGk=\n"] {
        for (method, id) in [("terminal/input", Some(9)), ("terminal/output", None)] {
            let mut value =
                json!({"method":method,"params":{"session":"claude:alpha","data":data}});
            if let Some(id) = id {
                value["id"] = json!(id);
            }
            assert!(
                matches!(decode_line(&value.to_string()), Err(DecodeError::InvalidParams { id:actual, .. }) if actual == id),
                "{value}"
            );
        }
    }
}

#[test]
fn request_classification_takes_precedence_when_method_is_present() {
    assert_eq!(
        decode_line(r#"{"id":4,"method":"sessions/list","result":{},"params":{}}"#),
        Ok(Message::Request {
            id: 4,
            request: ClientRequest::SessionsList
        })
    );
}

#[test]
fn decode_line_enforces_byte_limit_without_counting_line_terminators() {
    let prefix = "{\"id\":1,\"result\":\"";
    let suffix = "\"}";
    let line = format!(
        "{prefix}{}{suffix}",
        "a".repeat(MAX_LINE_BYTES - prefix.len() - suffix.len())
    );
    for ending in ["", "\n", "\r\n"] {
        assert!(decode_line(&format!("{line}{ending}")).is_ok());
    }
    assert_eq!(decode_line(&format!("{line} ")), Err(DecodeError::TooLong));
    assert_eq!(
        decode_line(&"é".repeat(MAX_LINE_BYTES / 2 + 1)),
        Err(DecodeError::TooLong)
    );
}

#[test]
fn line_reader_handles_empty_lines_crlf_and_final_unterminated_lines() {
    let mut reader = LineReader::new(Cursor::new(b"alpha\n\r\nbeta\r\nfinal"));
    for expected in ["alpha", "", "beta", "final"] {
        assert_eq!(reader.next_line().unwrap(), Some(expected.into()));
    }
    assert_eq!(reader.next_line().unwrap(), None);
    assert_eq!(reader.next_line().unwrap(), None);
    assert_eq!(LineReader::new(Cursor::new(b"")).next_line().unwrap(), None);
}

#[test]
fn line_reader_handles_utf8_and_crlf_across_buffer_boundaries() {
    for capacity in 1..=7 {
        let input = Cursor::new("界é\r\nalpha\n🦀".as_bytes());
        let mut reader = LineReader::new(BufReader::with_capacity(capacity, input));
        for expected in ["界é", "alpha", "🦀"] {
            assert_eq!(reader.next_line().unwrap(), Some(expected.into()));
        }
        assert_eq!(reader.next_line().unwrap(), None);
    }
}

#[test]
fn line_reader_accepts_exact_limit_and_rejects_larger_byte_lines() {
    for ending in ["", "\n", "\r\n"] {
        let input = format!("{}{ending}", "a".repeat(MAX_LINE_BYTES));
        let mut reader = LineReader::new(Cursor::new(input));
        assert_eq!(reader.next_line().unwrap().unwrap().len(), MAX_LINE_BYTES);
        assert!(reader.next_line().unwrap().is_none());
        let input = format!("{}{ending}", "a".repeat(MAX_LINE_BYTES + 1));
        assert!(matches!(
            LineReader::new(Cursor::new(input)).next_line(),
            Err(LineError::TooLong)
        ));
    }
    assert!(matches!(
        LineReader::new(Cursor::new("é".repeat(MAX_LINE_BYTES / 2 + 1))).next_line(),
        Err(LineError::TooLong)
    ));
}

struct FailingReader;
impl Read for FailingReader {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("invented read failure"))
    }
}
impl BufRead for FailingReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Err(io::Error::other("invented read failure"))
    }
    fn consume(&mut self, _: usize) {}
}

#[test]
fn line_reader_reports_utf8_and_io_errors() {
    assert!(matches!(
        LineReader::new(Cursor::new(vec![0xff, b'\n'])).next_line(),
        Err(LineError::InvalidUtf8)
    ));
    assert!(matches!(
        LineReader::new(Cursor::new(vec![0xc3])).next_line(),
        Err(LineError::InvalidUtf8)
    ));
    match LineReader::new(FailingReader).next_line() {
        Err(LineError::Io(error)) => assert_eq!(error.kind(), io::ErrorKind::Other),
        other => panic!("Expected an I/O error, got {other:?}"),
    }
}
