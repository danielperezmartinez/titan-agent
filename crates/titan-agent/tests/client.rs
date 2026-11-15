#![cfg(unix)]

use futures::channel::mpsc::UnboundedReceiver;
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use titan_agent::client::{ClientError, DaemonClient, Incoming, socket_from_env};
use titan_agent_core::protocol::{
    ClientMessage, ErrorBody, ErrorCode, Event, MAX_FRAME_BYTES, PROTOCOL_VERSION, Request,
    Response, ServerMessage, encode_line, read_frame,
};

const WAIT: Duration = Duration::from_secs(8);
const POLL: Duration = Duration::from_millis(5);
static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);

struct SocketDir(PathBuf);

impl SocketDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ta-client-{}-{}",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create unique socket directory");
        Self(path)
    }

    fn socket(&self) -> PathBuf {
        self.0.join("s")
    }
}

impl Drop for SocketDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn serve(
    socket: &Path,
    action: impl FnOnce(UnixStream) + Send + 'static,
) -> thread::JoinHandle<()> {
    let listener = UnixListener::bind(socket).expect("bind fake daemon");
    thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + WAIT;
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "client did not connect");
                    thread::sleep(POLL);
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        };
        stream.set_read_timeout(Some(WAIT)).unwrap();
        stream.set_write_timeout(Some(WAIT)).unwrap();
        action(stream);
    })
}

fn receive(reader: &mut BufReader<UnixStream>) -> ClientMessage {
    read_frame(reader)
        .expect("valid client frame")
        .expect("client message")
}

fn greet(stream: &mut UnixStream) {
    let hello = receive(&mut BufReader::new(stream.try_clone().unwrap()));
    assert_eq!(
        hello,
        ClientMessage {
            id: 1,
            request: Request::Hello {
                protocol: PROTOCOL_VERSION,
                client: "fixture-ui".into(),
            },
        }
    );
    stream
        .write_all(&encode_line(&ServerMessage::Response {
            id: 1,
            response: Response::Welcome {
                protocol: PROTOCOL_VERSION,
                daemon: "fixture-daemon-v7".into(),
            },
        }))
        .unwrap();
}

fn next(receiver: &mut UnboundedReceiver<Incoming>) -> Incoming {
    let deadline = Instant::now() + WAIT;
    loop {
        match receiver.try_next() {
            Ok(Some(message)) => return message,
            Ok(None) => panic!("incoming channel ended before expected message"),
            Err(_) => {
                assert!(Instant::now() < deadline, "incoming message timed out");
                thread::sleep(POLL);
            }
        }
    }
}

fn disconnected_once(receiver: &mut UnboundedReceiver<Incoming>) {
    assert!(matches!(next(receiver), Incoming::Disconnected(_)));
    let deadline = Instant::now() + Duration::from_millis(100);
    while Instant::now() < deadline {
        match receiver.try_next() {
            Ok(Some(message)) => panic!("message after Disconnected: {message:?}"),
            Ok(None) => return,
            Err(_) => thread::sleep(POLL),
        }
    }
}

fn wait_disconnected(client: &DaemonClient) {
    let deadline = Instant::now() + WAIT;
    while client.is_connected() {
        assert!(Instant::now() < deadline, "connection remained active");
        thread::sleep(POLL);
    }
}

fn error_body() -> ErrorBody {
    ErrorBody {
        code: ErrorCode::ProtocolMismatch,
        message: "fixture protocol rejected".into(),
    }
}

fn assert_error_traits(error: &ClientError) {
    fn is_error<T: std::error::Error>(_: &T) {}
    is_error(error);
    let _ = error.to_string();
}

#[test]
fn hello_uses_reserved_id_version_and_client_name_and_returns_daemon() {
    let dir = SocketDir::new();
    let (release, hold) = mpsc::channel();
    let daemon = serve(&dir.socket(), move |mut stream| {
        greet(&mut stream);
        hold.recv_timeout(WAIT).unwrap();
    });
    let (client, name, mut incoming) = DaemonClient::connect(&dir.socket(), "fixture-ui").unwrap();
    assert_eq!(name, "fixture-daemon-v7");
    assert!(client.is_connected());
    assert!(
        incoming.try_next().is_err(),
        "Hello answer must be consumed"
    );
    release.send(()).unwrap();
    disconnected_once(&mut incoming);
    wait_disconnected(&client);
    daemon.join().unwrap();
}

#[test]
fn hello_error_is_returned_as_rejected_with_original_body() {
    let dir = SocketDir::new();
    let daemon = serve(&dir.socket(), |mut stream| {
        receive(&mut BufReader::new(stream.try_clone().unwrap()));
        stream
            .write_all(&encode_line(&ServerMessage::Error {
                id: 1,
                error: error_body(),
            }))
            .unwrap();
    });
    match DaemonClient::connect(&dir.socket(), "fixture-ui") {
        Err(error) => {
            assert_error_traits(&error);
            assert!(matches!(error, ClientError::Rejected(body) if body == error_body()));
        }
        Ok(_) => panic!("rejected Hello accepted"),
    }
    daemon.join().unwrap();
}

#[test]
fn invalid_hello_answers_and_eof_are_handshake_errors() {
    let answers = [
        encode_line(&ServerMessage::Response {
            id: 1,
            response: Response::Done,
        }),
        encode_line(&ServerMessage::Event {
            event: Event::Sessions { sessions: vec![] },
        }),
        b"{invalid json}\n".to_vec(),
        vec![],
    ];
    for answer in answers {
        let dir = SocketDir::new();
        let daemon = serve(&dir.socket(), move |mut stream| {
            receive(&mut BufReader::new(stream.try_clone().unwrap()));
            stream.write_all(&answer).unwrap();
        });
        assert!(matches!(
            DaemonClient::connect(&dir.socket(), "fixture-ui"),
            Err(ClientError::Handshake(_))
        ));
        daemon.join().unwrap();
    }
}

#[test]
fn silent_daemon_hits_the_five_second_handshake_timeout() {
    assert_eq!(DaemonClient::TIMEOUT, Duration::from_secs(5));
    let dir = SocketDir::new();
    let (release, hold) = mpsc::channel();
    let daemon = serve(&dir.socket(), move |stream| {
        let mut reader = BufReader::new(stream);
        receive(&mut reader);
        // Keep the connection open until connect has returned.
        let _ = hold.recv_timeout(WAIT);
    });
    let start = Instant::now();
    let result = DaemonClient::connect(&dir.socket(), "fixture-ui");
    let elapsed = start.elapsed();
    release.send(()).unwrap();
    assert!(matches!(result, Err(ClientError::Handshake(_))));
    assert!(elapsed >= DaemonClient::TIMEOUT - Duration::from_millis(100));
    assert!(elapsed < DaemonClient::TIMEOUT + Duration::from_secs(2));
    daemon.join().unwrap();
}

#[test]
fn sequential_inputs_get_consecutive_ids_and_reach_daemon_in_fifo_order() {
    let dir = SocketDir::new();
    let daemon = serve(&dir.socket(), |mut stream| {
        greet(&mut stream);
        let mut reader = BufReader::new(stream);
        for index in 0..1_000_u64 {
            assert_eq!(
                receive(&mut reader),
                ClientMessage {
                    id: index + 2,
                    request: Request::Input {
                        terminal: 17,
                        data: format!("key-{index}").as_str().into()
                    },
                }
            );
        }
    });
    let (client, _, mut incoming) = DaemonClient::connect(&dir.socket(), "fixture-ui").unwrap();
    for index in 0..1_000_u64 {
        assert_eq!(
            client.send(Request::Input {
                terminal: 17,
                data: format!("key-{index}").as_str().into()
            }),
            index + 2
        );
    }
    daemon.join().unwrap();
    disconnected_once(&mut incoming);
}

#[test]
fn send_does_not_wait_for_a_daemon_that_is_not_reading() {
    let dir = SocketDir::new();
    let (release, hold) = mpsc::channel();
    let daemon = serve(&dir.socket(), move |mut stream| {
        greet(&mut stream);
        let _ = hold.recv_timeout(WAIT);
    });
    let (client, _, mut incoming) = DaemonClient::connect(&dir.socket(), "fixture-ui").unwrap();
    let (finished, completion) = mpsc::channel();
    let sender = thread::spawn(move || {
        // Enough data to fill a Unix socket buffer while the daemon does not read.
        for index in 0..64 {
            assert_eq!(
                client.send(Request::Input {
                    terminal: 17,
                    data: vec![b'x'; 128 * 1024].into()
                }),
                index + 2
            );
        }
        drop(client);
        finished.send(()).unwrap();
    });
    let outcome = completion.recv_timeout(Duration::from_secs(3));
    release.send(()).unwrap();
    assert!(
        outcome.is_ok(),
        "send or Drop blocked on socket backpressure"
    );
    sender.join().unwrap();
    disconnected_once(&mut incoming);
    daemon.join().unwrap();
}

#[test]
fn incoming_preserves_order_and_skips_invalid_json_then_reports_eof_once() {
    let dir = SocketDir::new();
    let event = Event::TerminalOutput {
        terminal: 17,
        data: "fixture output".into(),
    };
    let expected_event = event.clone();
    let daemon = serve(&dir.socket(), move |mut stream| {
        greet(&mut stream);
        for message in [
            ServerMessage::Response {
                id: 3,
                response: Response::Done,
            },
            ServerMessage::Error {
                id: 2,
                error: error_body(),
            },
            ServerMessage::Event { event },
        ] {
            stream.write_all(b"{broken}\n").unwrap();
            stream.write_all(&encode_line(&message)).unwrap();
        }
    });
    let (client, _, mut incoming) = DaemonClient::connect(&dir.socket(), "fixture-ui").unwrap();
    assert_eq!(
        next(&mut incoming),
        Incoming::Response {
            id: 3,
            response: Response::Done
        }
    );
    assert_eq!(
        next(&mut incoming),
        Incoming::Error {
            id: 2,
            error: error_body()
        }
    );
    assert_eq!(next(&mut incoming), Incoming::Event(expected_event));
    disconnected_once(&mut incoming);
    wait_disconnected(&client);
    assert_eq!(client.send(Request::Subscribe), 2);
    assert_eq!(client.send(Request::Shutdown), 3);
    assert!(!client.is_connected());
    daemon.join().unwrap();
}

#[test]
fn oversized_and_truncated_frames_disconnect_instead_of_being_skipped() {
    for bytes in [vec![b'x'; MAX_FRAME_BYTES + 1], b"{\"type\":".to_vec()] {
        let dir = SocketDir::new();
        let daemon = serve(&dir.socket(), move |mut stream| {
            greet(&mut stream);
            // A framing error may cause the client to close before all bytes are written.
            let _ = stream.write_all(&bytes);
        });
        let (client, _, mut incoming) = DaemonClient::connect(&dir.socket(), "fixture-ui").unwrap();
        disconnected_once(&mut incoming);
        wait_disconnected(&client);
        daemon.join().unwrap();
    }
}

#[test]
fn dropping_client_closes_socket_and_notifies_receiver_without_hanging() {
    let dir = SocketDir::new();
    let daemon = serve(&dir.socket(), |mut stream| {
        greet(&mut stream);
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).expect("daemon observes EOF"), 0);
    });
    let (client, _, mut incoming) = DaemonClient::connect(&dir.socket(), "fixture-ui").unwrap();
    let (finished, completion) = mpsc::channel();
    let dropper = thread::spawn(move || {
        drop(client);
        finished.send(()).unwrap();
    });
    completion
        .recv_timeout(Duration::from_secs(2))
        .expect("Drop must not hang");
    disconnected_once(&mut incoming);
    dropper.join().unwrap();
    daemon.join().unwrap();
}

#[test]
fn writer_failure_marks_connection_disconnected() {
    let dir = SocketDir::new();
    let (ready_sender, ready_receiver) = mpsc::channel();
    let (release, hold) = mpsc::channel();
    let daemon = serve(&dir.socket(), move |mut stream| {
        greet(&mut stream);
        stream.shutdown(Shutdown::Read).unwrap();
        ready_sender.send(()).unwrap();
        // Keep the write direction open so EOF cannot hide a writer failure.
        let _ = hold.recv_timeout(WAIT * 2);
    });
    let (client, _, mut incoming) = DaemonClient::connect(&dir.socket(), "fixture-ui").unwrap();
    ready_receiver.recv_timeout(WAIT).unwrap();
    assert_eq!(client.send(Request::Subscribe), 2);
    wait_disconnected(&client);
    assert_eq!(client.send(Request::Shutdown), 3);
    release.send(()).unwrap();
    disconnected_once(&mut incoming);
    daemon.join().unwrap();
}

#[test]
fn connect_to_missing_socket_returns_connect_error() {
    let dir = SocketDir::new();
    assert!(matches!(
        DaemonClient::connect(&dir.socket(), "fixture-ui"),
        Err(ClientError::Connect(_))
    ));
}

#[test]
fn non_retryable_connection_error_does_not_invoke_spawn() {
    let dir = SocketDir::new();
    let file = dir.0.join("regular-file");
    fs::write(&file, b"fixture").unwrap();
    let start = Instant::now();
    let result = DaemonClient::connect_or_spawn(&file.join("s"), "fixture-ui", || {
        panic!("spawn called for a path component that is not a directory");
    });
    assert!(matches!(result, Err(ClientError::Connect(_))));
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn connect_or_spawn_retries_until_delayed_daemon_is_ready_and_spawns_once() {
    let dir = SocketDir::new();
    let path = dir.socket();
    let calls = AtomicUsize::new(0);
    let (handle_sender, handle_receiver) = mpsc::channel();
    let (client, name, mut incoming) = DaemonClient::connect_or_spawn(&path, "fixture-ui", || {
        calls.fetch_add(1, Ordering::SeqCst);
        let socket = path.clone();
        let launcher = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            serve(&socket, |mut stream| greet(&mut stream))
                .join()
                .unwrap();
        });
        handle_sender.send(launcher).unwrap();
        Ok(())
    })
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(name, "fixture-daemon-v7");
    disconnected_once(&mut incoming);
    wait_disconnected(&client);
    handle_receiver.recv_timeout(WAIT).unwrap().join().unwrap();
}

#[test]
fn connect_or_spawn_handles_a_stale_socket_with_no_listener() {
    let dir = SocketDir::new();
    let socket = dir.socket();
    drop(UnixListener::bind(&socket).unwrap());
    let calls = AtomicUsize::new(0);
    let result = DaemonClient::connect_or_spawn(&socket, "fixture-ui", || {
        calls.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "fixture spawn denied",
        ))
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        matches!(result, Err(ClientError::Connect(error)) if error.kind() == io::ErrorKind::PermissionDenied)
    );
}

#[test]
fn spawn_failure_is_returned_immediately_as_connect_error() {
    let dir = SocketDir::new();
    let calls = AtomicUsize::new(0);
    let start = Instant::now();
    let result = DaemonClient::connect_or_spawn(&dir.socket(), "fixture-ui", || {
        calls.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "fixture spawn denied",
        ))
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    match result {
        Err(ClientError::Connect(error)) => {
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(error.to_string(), "fixture spawn denied");
        }
        _ => panic!("spawn error was not returned"),
    }
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn spawn_success_without_listener_times_out_as_connect_error() {
    let dir = SocketDir::new();
    let calls = AtomicUsize::new(0);
    let start = Instant::now();
    let result = DaemonClient::connect_or_spawn(&dir.socket(), "fixture-ui", || {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(matches!(result, Err(ClientError::Connect(_))));
    assert!(start.elapsed() >= DaemonClient::TIMEOUT - Duration::from_millis(100));
    assert!(start.elapsed() < DaemonClient::TIMEOUT + Duration::from_secs(2));
}

#[test]
fn existing_daemon_does_not_invoke_spawn() {
    let dir = SocketDir::new();
    let daemon = serve(&dir.socket(), |mut stream| greet(&mut stream));
    let (client, name, _) = DaemonClient::connect_or_spawn(&dir.socket(), "fixture-ui", || {
        panic!("spawn called with an existing daemon");
    })
    .unwrap();
    assert_eq!(name, "fixture-daemon-v7");
    drop(client);
    daemon.join().unwrap();
}

#[test]
fn handshake_rejection_does_not_invoke_spawn() {
    let dir = SocketDir::new();
    let daemon = serve(&dir.socket(), |mut stream| {
        receive(&mut BufReader::new(stream.try_clone().unwrap()));
        stream
            .write_all(&encode_line(&ServerMessage::Error {
                id: 1,
                error: error_body(),
            }))
            .unwrap();
    });
    let result = DaemonClient::connect_or_spawn(&dir.socket(), "fixture-ui", || {
        panic!("spawn called after handshake rejection");
    });
    assert!(matches!(result, Err(ClientError::Rejected(body)) if body == error_body()));
    daemon.join().unwrap();
}

#[test]
fn socket_environment_override_is_used() {
    const MARKER: &str = "TITAN_CLIENT_TEST_ENV_CHILD";
    if std::env::var_os(MARKER).is_some() {
        assert_eq!(
            socket_from_env(),
            PathBuf::from(std::env::var_os("TITAN_AGENT_SOCKET").unwrap())
        );
        return;
    }
    let dir = SocketDir::new();
    // Isolate environment changes in a subprocess, avoiding global mutation and unsafe code.
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "socket_environment_override_is_used",
            "--nocapture",
        ])
        .env(MARKER, "1")
        .env("TITAN_AGENT_SOCKET", dir.socket())
        .env("XDG_RUNTIME_DIR", dir.0.join("unused-runtime"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "environment subprocess failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
