#![cfg(target_os = "linux")]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;
use titan_agent::client::{
    ClientError, Connection, connect_or_spawn, daemon_program, default_socket_path,
};
use titan_agent_core::protocol::{
    ClientRequest, DaemonNotification, Message, PROTOCOL_VERSION, RpcError, decode_line,
    encode_line, error_code,
};

const WAIT: Duration = Duration::from_secs(5);
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "titan-ui-test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    fn socket(&self) -> PathBuf {
        self.0.join("fake.sock")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Peer {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Peer {
    fn accept(listener: UnixListener) -> Self {
        let (stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        stream.set_write_timeout(Some(WAIT)).unwrap();
        Self {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
        }
    }

    fn receive(&mut self) -> Message {
        let mut line = String::new();
        assert_ne!(self.reader.read_line(&mut line).unwrap(), 0);
        decode_line(&line).unwrap()
    }

    fn send(&mut self, message: Message) {
        self.writer
            .write_all(encode_line(&message).as_bytes())
            .unwrap();
    }

    fn read_hello(&mut self) -> u64 {
        match self.receive() {
            Message::Request {
                id,
                request: ClientRequest::Hello { protocol, client },
            } => {
                assert_eq!(id, 1);
                assert_eq!(protocol, PROTOCOL_VERSION);
                assert_eq!(client.name, "titan-agent");
                assert_eq!(client.version, env!("CARGO_PKG_VERSION"));
                id
            }
            other => panic!("expected hello, got {other:?}"),
        }
    }

    fn handshake(&mut self) {
        let id = self.read_hello();
        self.send(Message::Response {
            id,
            result: Ok(serde_json::json!({
                "protocol": PROTOCOL_VERSION,
                "daemon": { "name": "fake-agentd", "version": "0.0.0" }
            })),
        });
    }
}

fn request_with_deadline(
    connection: Arc<Connection>,
    request: ClientRequest,
) -> Result<serde_json::Value, ClientError> {
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = connection.request(request);
        let _ = sender.send(result);
    });
    let result = receiver.recv_timeout(WAIT).expect("request did not finish");
    worker.join().unwrap();
    result
}

#[test]
fn connection_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Connection>();
}

#[test]
fn hello_is_first_and_sequential_requests_use_increasing_ids() {
    let scratch = Scratch::new();
    let socket = scratch.socket();
    let listener = UnixListener::bind(&socket).unwrap();
    let server = thread::spawn(move || {
        let mut peer = Peer::accept(listener);
        peer.handshake();
        for (expected_id, expected_request, value) in [
            (
                2,
                ClientRequest::SessionsSubscribe,
                serde_json::json!({"sessions": []}),
            ),
            (
                3,
                ClientRequest::TerminalOpen {
                    session: "session-a".into(),
                    cols: 80,
                    rows: 24,
                },
                serde_json::json!({}),
            ),
            (
                4,
                ClientRequest::TerminalInput {
                    session: "session-a".into(),
                    data: vec![0, 27, 255],
                },
                serde_json::json!({}),
            ),
            (
                5,
                ClientRequest::TerminalResize {
                    session: "session-a".into(),
                    cols: 100,
                    rows: 30,
                },
                serde_json::json!({}),
            ),
            (
                6,
                ClientRequest::TerminalClose {
                    session: "session-a".into(),
                },
                serde_json::json!({}),
            ),
            (
                7,
                ClientRequest::SessionStop {
                    session: "session-a".into(),
                },
                serde_json::json!({}),
            ),
        ] {
            assert_eq!(
                peer.receive(),
                Message::Request {
                    id: expected_id,
                    request: expected_request
                }
            );
            peer.send(Message::Response {
                id: expected_id,
                result: Ok(value),
            });
        }
    });
    let connection = Arc::new(Connection::connect(&socket).unwrap());
    for (request, expected) in [
        (
            ClientRequest::SessionsSubscribe,
            serde_json::json!({"sessions": []}),
        ),
        (
            ClientRequest::TerminalOpen {
                session: "session-a".into(),
                cols: 80,
                rows: 24,
            },
            serde_json::json!({}),
        ),
        (
            ClientRequest::TerminalInput {
                session: "session-a".into(),
                data: vec![0, 27, 255],
            },
            serde_json::json!({}),
        ),
        (
            ClientRequest::TerminalResize {
                session: "session-a".into(),
                cols: 100,
                rows: 30,
            },
            serde_json::json!({}),
        ),
        (
            ClientRequest::TerminalClose {
                session: "session-a".into(),
            },
            serde_json::json!({}),
        ),
        (
            ClientRequest::SessionStop {
                session: "session-a".into(),
            },
            serde_json::json!({}),
        ),
    ] {
        assert_eq!(
            request_with_deadline(connection.clone(), request).unwrap(),
            expected
        );
    }
    server.join().unwrap();
}

#[test]
fn daemon_hello_error_is_preserved_as_rpc_error() {
    let scratch = Scratch::new();
    let socket = scratch.socket();
    let listener = UnixListener::bind(&socket).unwrap();
    let expected = RpcError {
        code: error_code::UNSUPPORTED_PROTOCOL,
        message: "unsupported test protocol".into(),
    };
    let error = expected.clone();
    let server = thread::spawn(move || {
        let mut peer = Peer::accept(listener);
        let id = peer.read_hello();
        peer.send(Message::Response {
            id,
            result: Err(error),
        });
    });
    match Connection::connect(&socket) {
        Err(ClientError::Rpc(error)) => assert_eq!(error, expected),
        Err(other) => panic!("expected RPC error, got {other:?}"),
        Ok(_) => panic!("hello error must reject the connection"),
    }
    server.join().unwrap();
}

#[test]
fn concurrent_requests_match_reversed_responses_and_notifications_keep_arrival_order() {
    let scratch = Scratch::new();
    let socket = scratch.socket();
    let listener = UnixListener::bind(&socket).unwrap();
    let notifications = vec![
        DaemonNotification::SessionsChanged { sessions: vec![] },
        DaemonNotification::TerminalOutput {
            session: "session-a".into(),
            data: vec![0, 27, 255],
        },
        DaemonNotification::TerminalExited {
            session: "session-a".into(),
            code: Some(7),
        },
    ];
    let outgoing = notifications.clone();
    let server = thread::spawn(move || {
        let mut peer = Peer::accept(listener);
        peer.handshake();
        // Queue a notification before the consumer takes its receiver.
        peer.send(Message::Notification(outgoing[0].clone()));
        let mut pending = Vec::new();
        for _ in 0..8 {
            match peer.receive() {
                Message::Request {
                    id,
                    request: ClientRequest::TerminalClose { session },
                } => pending.push((id, session)),
                other => panic!("unexpected request: {other:?}"),
            }
        }
        let mut ids: Vec<_> = pending.iter().map(|(id, _)| *id).collect();
        ids.sort_unstable();
        assert_eq!(ids, (2..=9).collect::<Vec<_>>());
        for (position, (id, session)) in pending.into_iter().rev().enumerate() {
            if position == 1 {
                peer.send(Message::Notification(outgoing[1].clone()));
            }
            peer.send(Message::Response {
                id,
                result: Ok(serde_json::json!({"session": session})),
            });
            if position == 4 {
                peer.send(Message::Notification(outgoing[2].clone()));
            }
        }
    });
    let connection = Arc::new(Connection::connect(&socket).unwrap());
    let receiver = connection.take_notifications().expect("first receiver");
    assert!(connection.take_notifications().is_none());
    let (done, results) = mpsc::channel();
    let mut workers = Vec::new();
    for index in 0..8 {
        let connection = connection.clone();
        let done = done.clone();
        workers.push(thread::spawn(move || {
            let session = format!("session-{index}");
            let result = connection.request(ClientRequest::TerminalClose {
                session: session.clone(),
            });
            let _ = done.send((session, result));
        }));
    }
    drop(done);
    for _ in 0..8 {
        let (session, result) = results
            .recv_timeout(WAIT)
            .expect("concurrent request stalled");
        assert_eq!(result.unwrap(), serde_json::json!({"session": session}));
    }
    for expected in notifications {
        assert_eq!(receiver.recv_timeout(WAIT).unwrap(), expected);
    }
    assert_eq!(
        receiver.recv_timeout(WAIT),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
    for worker in workers {
        worker.join().unwrap();
    }
    server.join().unwrap();
}

#[test]
fn ordinary_rpc_errors_preserve_unknown_and_unavailable_session_errors() {
    let scratch = Scratch::new();
    let socket = scratch.socket();
    let listener = UnixListener::bind(&socket).unwrap();
    let server = thread::spawn(move || {
        let mut peer = Peer::accept(listener);
        peer.handshake();
        for code in [error_code::UNKNOWN_SESSION, error_code::NOT_AVAILABLE] {
            let Message::Request { id, .. } = peer.receive() else {
                panic!("expected request")
            };
            peer.send(Message::Response {
                id,
                result: Err(RpcError {
                    code,
                    message: "test session error".into(),
                }),
            });
        }
    });
    let connection = Arc::new(Connection::connect(&socket).unwrap());
    for code in [error_code::UNKNOWN_SESSION, error_code::NOT_AVAILABLE] {
        match request_with_deadline(
            connection.clone(),
            ClientRequest::TerminalOpen {
                session: "missing-session".into(),
                cols: 80,
                rows: 24,
            },
        ) {
            Err(ClientError::Rpc(error)) => assert_eq!(
                error,
                RpcError {
                    code,
                    message: "test session error".into()
                }
            ),
            other => panic!("expected RPC error, got {other:?}"),
        }
    }
    server.join().unwrap();
}

#[test]
fn socket_closure_releases_pending_and_future_requests_and_closes_notifications() {
    let scratch = Scratch::new();
    let socket = scratch.socket();
    let listener = UnixListener::bind(&socket).unwrap();
    let server = thread::spawn(move || {
        let mut peer = Peer::accept(listener);
        peer.handshake();
        // Wait for all requests, then drop both halves without answering any.
        for _ in 0..3 {
            assert!(matches!(peer.receive(), Message::Request { .. }));
        }
    });
    let connection = Arc::new(Connection::connect(&socket).unwrap());
    let notifications = connection.take_notifications().unwrap();
    let (done, results) = mpsc::channel();
    let mut workers = Vec::new();
    for _ in 0..3 {
        let connection = connection.clone();
        let done = done.clone();
        workers.push(thread::spawn(move || {
            let _ = done.send(connection.request(ClientRequest::SessionsSubscribe));
        }));
    }
    drop(done);
    for _ in 0..3 {
        assert!(matches!(
            results.recv_timeout(WAIT).unwrap(),
            Err(ClientError::Closed)
        ));
    }
    assert_eq!(
        notifications.recv_timeout(WAIT),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
    assert!(matches!(
        request_with_deadline(connection, ClientRequest::SessionsSubscribe),
        Err(ClientError::Closed)
    ));
    for worker in workers {
        worker.join().unwrap();
    }
    server.join().unwrap();
}

#[test]
fn missing_socket_returns_io_and_missing_daemon_program_returns_an_error() {
    let scratch = Scratch::new();
    assert!(matches!(
        Connection::connect(&scratch.socket()),
        Err(ClientError::Io(_))
    ));
    assert!(
        connect_or_spawn(
            &scratch.socket(),
            &scratch.0.join("absent-agentd"),
            Duration::from_millis(200)
        )
        .is_err()
    );
}

#[test]
fn connect_or_spawn_uses_an_existing_daemon_without_needing_a_program() {
    let scratch = Scratch::new();
    let socket = scratch.socket();
    let listener = UnixListener::bind(&socket).unwrap();
    let server = thread::spawn(move || Peer::accept(listener).handshake());
    assert!(connect_or_spawn(&socket, &scratch.0.join("absent-agentd"), WAIT).is_ok());
    server.join().unwrap();
}

#[test]
fn connect_or_spawn_times_out_when_the_started_program_never_opens_a_socket() {
    let scratch = Scratch::new();
    let program = scratch.0.join("idle-agentd");
    fs::write(&program, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        connect_or_spawn(&scratch.socket(), &program, Duration::from_millis(250)),
        Err(ClientError::Timeout)
    ));
}

#[test]
fn daemon_program_prefers_the_executable_sibling_when_present() {
    let candidate = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("titan-agentd");
    let expected = if candidate.exists() {
        candidate
    } else {
        PathBuf::from("titan-agentd")
    };
    assert_eq!(daemon_program(), expected);
}

#[test]
fn socket_path_environment_rules() {
    const CHILD_CASE: &str = "TITAN_ACCEPTANCE_SOCKET_CASE";
    if let Ok(case) = std::env::var(CHILD_CASE) {
        let expected = match case.as_str() {
            "explicit" => PathBuf::from("/fictional-titan/override.sock"),
            "runtime" | "empty-override" => {
                PathBuf::from("/fictional-runtime/titan-agent/agentd.sock")
            }
            "fallback" | "empty-runtime" => {
                let uid = fs::metadata("/proc/self").unwrap().uid();
                std::env::temp_dir().join(format!("titan-agent-{uid}/agentd.sock"))
            }
            _ => panic!("unknown child case"),
        };
        assert_eq!(default_socket_path(), expected);
        return;
    }
    // Rust 2024 environment mutations require unsafe; isolated child environments do not.
    for case in [
        "explicit",
        "runtime",
        "empty-override",
        "fallback",
        "empty-runtime",
    ] {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args(["--exact", "socket_path_environment_rules", "--nocapture"])
            .env(CHILD_CASE, case)
            .env_remove("TITAN_AGENTD_SOCKET")
            .env_remove("XDG_RUNTIME_DIR");
        match case {
            "explicit" => {
                child
                    .env("TITAN_AGENTD_SOCKET", "/fictional-titan/override.sock")
                    .env("XDG_RUNTIME_DIR", "/fictional-runtime");
            }
            "runtime" => {
                child.env("XDG_RUNTIME_DIR", "/fictional-runtime");
            }
            "empty-override" => {
                child
                    .env("TITAN_AGENTD_SOCKET", "")
                    .env("XDG_RUNTIME_DIR", "/fictional-runtime");
            }
            "empty-runtime" => {
                child
                    .env("TITAN_AGENTD_SOCKET", "")
                    .env("XDG_RUNTIME_DIR", "");
            }
            "fallback" => {}
            _ => unreachable!(),
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "case {case}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn spawned_daemon_fixture() {
    let Some(directory) = std::env::var_os("TITAN_ACCEPTANCE_SPAWN_DIRECTORY") else {
        return;
    };
    let directory = PathBuf::from(directory);
    let stat = fs::read_to_string("/proc/self/stat").unwrap();
    let fields: Vec<_> = stat[stat.rfind(')').unwrap() + 1..]
        .split_whitespace()
        .collect();
    let group: u32 = fields[2].parse().unwrap();
    let descriptors: Vec<_> = (0..3)
        .map(|fd| fs::read_link(format!("/proc/self/fd/{fd}")).unwrap())
        .collect();
    fs::write(
        directory.join("spawn-report.json"),
        serde_json::to_vec(&serde_json::json!({
            "pid": std::process::id(), "group": group, "descriptors": descriptors,
        }))
        .unwrap(),
    )
    .unwrap();
    // Force the caller to retry while the spawned daemon is starting.
    thread::sleep(Duration::from_millis(150));
    let listener = UnixListener::bind(directory.join("fake.sock")).unwrap();
    Peer::accept(listener).handshake();
}

#[test]
fn spawned_daemon_connects_after_startup_with_null_stdio_and_its_own_process_group() {
    fn shell_quote(value: &std::ffi::OsStr) -> String {
        format!("'{}'", value.to_str().unwrap().replace('\'', "'\\''"))
    }

    let scratch = Scratch::new();
    let program = scratch.0.join("fake-agentd");
    let executable = std::env::current_exe().unwrap();
    let script = format!(
        "#!/bin/sh\nexport TITAN_ACCEPTANCE_SPAWN_DIRECTORY={}\nexec {} --exact spawned_daemon_fixture --nocapture\n",
        shell_quote(scratch.0.as_os_str()),
        shell_quote(executable.as_os_str()),
    );
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    let connection = connect_or_spawn(&scratch.socket(), &program, WAIT).unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(scratch.0.join("spawn-report.json")).unwrap()).unwrap();
    assert_eq!(report["group"], report["pid"]);
    assert_eq!(
        report["descriptors"],
        serde_json::json!(["/dev/null", "/dev/null", "/dev/null"])
    );
    drop(connection);
}
