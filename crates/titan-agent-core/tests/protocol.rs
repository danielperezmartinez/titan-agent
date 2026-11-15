use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::io::{self, BufReader, Cursor, Read};
use titan_agent_core::protocol::{
    Bytes, ClientMessage, ErrorBody, ErrorCode, Event, FrameError, MAX_FRAME_BYTES,
    PROTOCOL_VERSION, Request, Response, ServerMessage, TerminalId, decode_line, encode_line,
    read_frame,
};
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

fn key() -> SessionKey {
    SessionKey::new(AgentCli::ClaudeCode, "sample-1")
}

fn session() -> Session {
    Session {
        key: key(),
        name: "worker".into(),
        kind: SessionKind::Interactive,
        state: SessionState::Working,
        cwd: "/work/sample".into(),
        project: Project {
            name: "sample".into(),
            root: "/work/sample".into(),
        },
        model: None,
        parent: None,
        started_at_ms: None,
        state_since_ms: None,
        attach: None,
    }
}

fn session_json() -> Value {
    json!({
        "key": {"cli": "claude", "id": "sample-1"}, "name": "worker",
        "kind": "interactive", "state": "working", "cwd": "/work/sample",
        "project": {"name": "sample", "root": "/work/sample"},
        "model": null, "parent": null, "started_at_ms": null, "state_since_ms": null,
        "attach": null
    })
}

#[test]
fn protocol_constants_and_terminal_id_match_the_contract() {
    assert_eq!(PROTOCOL_VERSION, 1_u32);
    assert_eq!(MAX_FRAME_BYTES, 8 * 1024 * 1024);
    let terminal: TerminalId = u64::MAX;
    assert_eq!(terminal, u64::MAX);
}

#[test]
fn bytes_use_standard_padded_base64_and_preserve_binary_data() {
    for (raw, encoded) in [
        (vec![], ""),
        (vec![0], "AA=="),
        (vec![0, 1], "AAE="),
        (vec![0, 1, 2], "AAEC"),
        (vec![251, 255, 255], "+///"),
        (b"hi".to_vec(), "aGk="),
    ] {
        assert_wire(Bytes(raw), json!(encoded));
    }
    let all_bytes: Vec<u8> = (0..=255).collect();
    let value = Bytes::from(all_bytes.clone());
    assert_eq!(value.0, all_bytes);
    assert_eq!(
        serde_json::from_slice::<Bytes>(&serde_json::to_vec(&value).unwrap()).unwrap(),
        value
    );
    assert_eq!(Bytes::default(), Bytes(vec![]));
    assert_eq!(Bytes::from(&b"hi"[..]), Bytes(b"hi".to_vec()));
    assert_eq!(Bytes::from("λ\n"), Bytes("λ\n".as_bytes().to_vec()));
}

#[test]
fn invalid_base64_and_non_string_bytes_are_rejected() {
    for invalid in [
        json!("!"),
        json!("aGk"),
        json!("aGk==="),
        json!("-___"),
        json!("aGk=\n"),
        json!("AB=="),
        json!(null),
        json!([104, 105]),
        json!(42),
    ] {
        assert!(
            serde_json::from_value::<Bytes>(invalid.clone()).is_err(),
            "accepted {invalid}"
        );
    }
}

#[test]
fn every_client_request_has_the_declared_shape() {
    let session = json!({"cli": "claude", "id": "sample-1"});
    let cases = [
        (
            Request::Hello {
                protocol: 1,
                client: "sample-ui 0.2.0".into(),
            },
            json!({"kind":"hello","protocol":1,"client":"sample-ui 0.2.0"}),
        ),
        (Request::Subscribe, json!({"kind":"subscribe"})),
        (
            Request::OpenTerminal {
                session: key(),
                cols: 120,
                rows: 40,
            },
            json!({"kind":"open_terminal","session":session,"cols":120,"rows":40}),
        ),
        (
            Request::Input {
                terminal: 3,
                data: Bytes::from("hi"),
            },
            json!({"kind":"input","terminal":3,"data":"aGk="}),
        ),
        (
            Request::Resize {
                terminal: 3,
                cols: 80,
                rows: 24,
            },
            json!({"kind":"resize","terminal":3,"cols":80,"rows":24}),
        ),
        (
            Request::Detach { terminal: 3 },
            json!({"kind":"detach","terminal":3}),
        ),
        (
            Request::CloseTerminal { terminal: 3 },
            json!({"kind":"close_terminal","terminal":3}),
        ),
        (
            Request::StopSession { session: key() },
            json!({"kind":"stop_session","session":session}),
        ),
        (Request::Shutdown, json!({"kind":"shutdown"})),
    ];
    for (index, (request, expected)) in cases.into_iter().enumerate() {
        assert_wire(request.clone(), expected.clone());
        let id = index as u64 + 1;
        assert_wire(
            ClientMessage { id, request },
            json!({"id":id,"request":expected}),
        );
    }
}

#[test]
fn every_response_has_the_declared_server_envelope() {
    let cases = [
        (
            Response::Welcome {
                protocol: 1,
                daemon: "sample-daemon 0.2.0".into(),
            },
            json!({"kind":"welcome","protocol":1,"daemon":"sample-daemon 0.2.0"}),
        ),
        (
            Response::Sessions {
                sessions: vec![session()],
            },
            json!({"kind":"sessions","sessions":[session_json()]}),
        ),
        (
            Response::Sessions { sessions: vec![] },
            json!({"kind":"sessions","sessions":[]}),
        ),
        (
            Response::TerminalOpened {
                terminal: 3,
                session: key(),
                cols: 120,
                rows: 40,
                reused: false,
            },
            json!({"kind":"terminal_opened","terminal":3,"session":{"cli":"claude","id":"sample-1"},"cols":120,"rows":40,"reused":false}),
        ),
        (
            Response::TerminalOpened {
                terminal: 3,
                session: key(),
                cols: 120,
                rows: 40,
                reused: true,
            },
            json!({"kind":"terminal_opened","terminal":3,"session":{"cli":"claude","id":"sample-1"},"cols":120,"rows":40,"reused":true}),
        ),
        (Response::Done, json!({"kind":"done"})),
    ];
    for (response, expected) in cases {
        assert_wire(response.clone(), expected.clone());
        assert_wire(
            ServerMessage::Response { id: 9, response },
            json!({"type":"response","id":9,"response":expected}),
        );
    }
}

#[test]
fn every_event_has_the_declared_server_envelope() {
    let cases = [
        (
            Event::Sessions {
                sessions: vec![session()],
            },
            json!({"kind":"sessions","sessions":[session_json()]}),
        ),
        (
            Event::Sessions { sessions: vec![] },
            json!({"kind":"sessions","sessions":[]}),
        ),
        (
            Event::TerminalSnapshot {
                terminal: 3,
                cols: 120,
                rows: 40,
                data: Bytes::from("hi"),
            },
            json!({"kind":"terminal_snapshot","terminal":3,"cols":120,"rows":40,"data":"aGk="}),
        ),
        (
            Event::TerminalOutput {
                terminal: 3,
                data: Bytes::from("hi"),
            },
            json!({"kind":"terminal_output","terminal":3,"data":"aGk="}),
        ),
        (
            Event::TerminalExited {
                terminal: 3,
                code: None,
            },
            json!({"kind":"terminal_exited","terminal":3,"code":null}),
        ),
        (
            Event::TerminalExited {
                terminal: 3,
                code: Some(-1),
            },
            json!({"kind":"terminal_exited","terminal":3,"code":-1}),
        ),
        (
            Event::TerminalExited {
                terminal: 3,
                code: Some(0),
            },
            json!({"kind":"terminal_exited","terminal":3,"code":0}),
        ),
    ];
    for (event, expected) in cases {
        assert_wire(event.clone(), expected.clone());
        assert_wire(
            ServerMessage::Event { event },
            json!({"type":"event","event":expected}),
        );
    }
}

#[test]
fn every_error_code_has_the_declared_wire_name() {
    let cases = [
        (ErrorCode::ProtocolMismatch, "protocol_mismatch"),
        (ErrorCode::NotGreeted, "not_greeted"),
        (ErrorCode::BadRequest, "bad_request"),
        (ErrorCode::UnknownSession, "unknown_session"),
        (ErrorCode::NotAttachable, "not_attachable"),
        (ErrorCode::UnknownTerminal, "unknown_terminal"),
        (ErrorCode::SpawnFailed, "spawn_failed"),
        (ErrorCode::StopFailed, "stop_failed"),
        (ErrorCode::Unsupported, "unsupported"),
        (ErrorCode::Internal, "internal"),
    ];
    let mut unique = std::collections::HashSet::new();
    for (code, name) in cases {
        unique.insert(code);
        assert_wire(code, json!(name));
        let error = ErrorBody {
            code,
            message: "sample failure".into(),
        };
        let expected = json!({"code":name,"message":"sample failure"});
        assert_wire(error.clone(), expected.clone());
        assert_wire(
            ServerMessage::Error { id: 7, error },
            json!({"type":"error","id":7,"error":expected}),
        );
    }
    assert_eq!(unique.len(), 10);
    assert!(serde_json::from_value::<ErrorCode>(json!("other")).is_err());
}

#[test]
fn numeric_fields_accept_their_type_boundaries_without_daemon_validation() {
    assert_wire(
        ClientMessage {
            id: u64::MAX,
            request: Request::Hello {
                protocol: u32::MAX,
                client: String::new(),
            },
        },
        json!({"id":u64::MAX,"request":{"kind":"hello","protocol":u32::MAX,"client":""}}),
    );
    assert_wire(
        Request::Resize {
            terminal: u64::MAX,
            cols: 0,
            rows: u16::MAX,
        },
        json!({"kind":"resize","terminal":u64::MAX,"cols":0,"rows":u16::MAX}),
    );
    assert_wire(
        ClientMessage {
            id: 0,
            request: Request::Subscribe,
        },
        json!({"id":0,"request":{"kind":"subscribe"}}),
    );
    for code in [i32::MIN, i32::MAX] {
        assert_wire(
            Event::TerminalExited {
                terminal: 0,
                code: Some(code),
            },
            json!({"kind":"terminal_exited","terminal":0,"code":code}),
        );
    }
}

#[test]
fn malformed_tags_missing_fields_and_out_of_range_numbers_are_rejected() {
    for invalid in [
        json!({"kind":"other"}),
        json!({}),
        json!({"kind":"hello","client":"sample"}),
        json!({"kind":"input","terminal":3,"data":"!"}),
        json!({"kind":"resize","terminal":-1,"cols":80,"rows":24}),
        json!({"kind":"resize","terminal":3,"cols":65536,"rows":24}),
        json!({"kind":"resize","terminal":3,"cols":80,"rows":-1}),
        json!({"kind":"hello","protocol":4294967296_u64,"client":"sample"}),
    ] {
        assert!(
            serde_json::from_value::<Request>(invalid.clone()).is_err(),
            "accepted {invalid}"
        );
    }
    assert!(
        serde_json::from_value::<ClientMessage>(json!({"request":{"kind":"subscribe"}})).is_err()
    );
    assert!(
        serde_json::from_value::<ClientMessage>(json!({"id":-1,"request":{"kind":"subscribe"}}))
            .is_err()
    );
    assert!(serde_json::from_value::<ServerMessage>(json!({"type":"other"})).is_err());
    assert!(serde_json::from_value::<ServerMessage>(json!({"type":"response","id":1})).is_err());
    assert!(serde_json::from_value::<Response>(json!({"kind":"other"})).is_err());
    assert!(serde_json::from_value::<Event>(json!({"kind":"other"})).is_err());
    assert!(
        serde_json::from_value::<Event>(
            json!({"kind":"terminal_exited","terminal":3,"code":2147483648_u64})
        )
        .is_err()
    );
}

#[test]
fn encode_line_is_compact_json_with_one_terminating_newline() {
    let value = json!({"text":"first\nsecond\r\nλ", "items":[1,2]});
    let encoded = encode_line(&value);
    assert_eq!(encoded.last(), Some(&b'\n'));
    assert_eq!(encoded.iter().filter(|&&byte| byte == b'\n').count(), 1);
    let payload = &encoded[..encoded.len() - 1];
    assert_eq!(payload, serde_json::to_vec(&value).unwrap());
    assert_eq!(serde_json::from_slice::<Value>(payload).unwrap(), value);
}

#[test]
fn decode_line_accepts_bare_lf_and_crlf_frames() {
    for suffix in ["", "\n", "\r\n"] {
        let input = format!("{{\"id\":2,\"request\":{{\"kind\":\"subscribe\"}}}}{suffix}");
        assert_eq!(
            decode_line::<ClientMessage>(input.as_bytes()).unwrap(),
            ClientMessage {
                id: 2,
                request: Request::Subscribe
            }
        );
    }
    let original = ServerMessage::Event {
        event: Event::TerminalOutput {
            terminal: 3,
            data: Bytes(vec![0, 255, 10]),
        },
    };
    assert_eq!(
        decode_line::<ServerMessage>(&encode_line(&original)).unwrap(),
        original
    );
}

#[test]
fn decode_line_reports_json_errors_for_invalid_json_utf8_and_types() {
    for input in [
        &b""[..],
        b"  ",
        b"{",
        b"\xff",
        b"1 2",
        b"null",
        b"{\"id\":1,\"request\":{\"kind\":\"other\"}}",
    ] {
        assert!(matches!(
            decode_line::<ClientMessage>(input),
            Err(FrameError::Json(_))
        ));
    }
}

#[test]
fn read_frame_skips_blank_lines_and_handles_multiple_buffer_boundaries() {
    let stream = b"\n \t\r\n\r\n {\"id\":1,\"request\":{\"kind\":\"subscribe\"}}\r\n{\"id\":2,\"request\":{\"kind\":\"shutdown\"}}\n \t\n";
    for capacity in [1, 2, 7, 4096] {
        let mut reader = BufReader::with_capacity(capacity, Cursor::new(stream));
        assert_eq!(
            read_frame::<_, ClientMessage>(&mut reader).unwrap(),
            Some(ClientMessage {
                id: 1,
                request: Request::Subscribe
            })
        );
        assert_eq!(
            read_frame::<_, ClientMessage>(&mut reader).unwrap(),
            Some(ClientMessage {
                id: 2,
                request: Request::Shutdown
            })
        );
        assert_eq!(read_frame::<_, ClientMessage>(&mut reader).unwrap(), None);
        assert_eq!(read_frame::<_, ClientMessage>(&mut reader).unwrap(), None);
    }
}

#[test]
fn clean_eof_and_unterminated_lines_are_distinct() {
    let mut empty = Cursor::new(b"");
    assert_eq!(read_frame::<_, Value>(&mut empty).unwrap(), None);
    for input in [&b"{}"[..], b"{", b" \t", b"\r", b"{}\r"] {
        let mut reader = Cursor::new(input);
        assert!(matches!(
            read_frame::<_, Value>(&mut reader),
            Err(FrameError::UnexpectedEof)
        ));
    }
    let mut reader = Cursor::new(b"{}\n{\"pending\":true}");
    assert_eq!(
        read_frame::<_, Value>(&mut reader).unwrap(),
        Some(json!({}))
    );
    assert!(matches!(
        read_frame::<_, Value>(&mut reader),
        Err(FrameError::UnexpectedEof)
    ));
}

#[test]
fn json_errors_consume_one_line_and_allow_reading_the_next_frame() {
    let mut reader = BufReader::with_capacity(
        3,
        Cursor::new(
            b"{broken}\n\xff\n{\"id\":1}\n{\"id\":2,\"request\":{\"kind\":\"subscribe\"}}\n",
        ),
    );
    for _ in 0..3 {
        assert!(matches!(
            read_frame::<_, ClientMessage>(&mut reader),
            Err(FrameError::Json(_))
        ));
    }
    assert_eq!(
        read_frame::<_, ClientMessage>(&mut reader).unwrap(),
        Some(ClientMessage {
            id: 2,
            request: Request::Subscribe
        })
    );
    assert_eq!(read_frame::<_, ClientMessage>(&mut reader).unwrap(), None);
}

#[test]
fn maximum_sized_frame_excludes_the_newline() {
    let payload = "x".repeat(MAX_FRAME_BYTES - 2);
    let mut stream = Vec::with_capacity(MAX_FRAME_BYTES + 1);
    stream.push(b'"');
    stream.extend_from_slice(payload.as_bytes());
    stream.extend_from_slice(b"\"\n");
    let mut reader = BufReader::new(Cursor::new(stream));
    assert_eq!(read_frame::<_, String>(&mut reader).unwrap(), Some(payload));
    assert_eq!(read_frame::<_, String>(&mut reader).unwrap(), None);
}

#[test]
fn oversized_lines_are_rejected_even_without_a_newline_or_valid_json() {
    for terminator in [&b""[..], b"\n"] {
        let mut stream = vec![b'x'; MAX_FRAME_BYTES + 1];
        stream.extend_from_slice(terminator);
        let mut reader = BufReader::new(Cursor::new(stream));
        assert!(matches!(
            read_frame::<_, Value>(&mut reader),
            Err(FrameError::TooLarge)
        ));
    }
    let mut whitespace = Cursor::new(vec![b' '; MAX_FRAME_BYTES + 1]);
    assert!(matches!(
        read_frame::<_, Value>(&mut whitespace),
        Err(FrameError::TooLarge)
    ));
}

// The source fails immediately after the first excess byte. Waiting for a
// newline or another byte would incorrectly return Io instead of TooLarge.
struct LimitedSource {
    remaining: usize,
}

impl Read for LimitedSource {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            return Err(io::Error::other("sample read failure"));
        }
        let count = self.remaining.min(buffer.len());
        buffer[..count].fill(b'x');
        self.remaining -= count;
        Ok(count)
    }
}

#[test]
fn oversized_frame_is_reported_as_soon_as_the_limit_is_exceeded() {
    let mut reader = BufReader::with_capacity(
        1,
        LimitedSource {
            remaining: MAX_FRAME_BYTES + 1,
        },
    );
    assert!(matches!(
        read_frame::<_, Value>(&mut reader),
        Err(FrameError::TooLarge)
    ));
}

#[test]
fn reader_io_errors_are_preserved_and_frame_errors_implement_standard_traits() {
    fn assert_error<T: std::error::Error>() {}
    assert_error::<FrameError>();
    let mut reader = BufReader::new(LimitedSource { remaining: 0 });
    match read_frame::<_, Value>(&mut reader) {
        Err(FrameError::Io(error)) => {
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert_eq!(error.to_string(), "sample read failure");
        }
        other => panic!("expected Io, got {other:?}"),
    }
    let error: FrameError = io::Error::new(io::ErrorKind::PermissionDenied, "sample denial").into();
    assert!(
        matches!(error, FrameError::Io(ref inner) if inner.kind() == io::ErrorKind::PermissionDenied)
    );
    for error in [
        error,
        FrameError::TooLarge,
        FrameError::UnexpectedEof,
        FrameError::Json("sample JSON error".into()),
    ] {
        assert!(!error.to_string().is_empty());
        assert!(!format!("{error:?}").is_empty());
    }
}
