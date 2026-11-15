#![cfg(unix)]

use std::collections::VecDeque;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use titan_agent_core::protocol::{
    Bytes, ClientMessage, ErrorCode, Event, MAX_FRAME_BYTES, PROTOCOL_VERSION, Request, Response,
    ServerMessage, decode_line, encode_line,
};
use titan_agent_core::session::{
    AgentCli, Project, Session, SessionKey, SessionKind, SessionState,
};
use titan_agentd::server::{Daemon, DaemonConfig, DaemonError, ShutdownHandle};
use titan_agentd::snapshot::Emulator;
use titan_agentd::source::{EmptySource, FixtureSource, SessionSource, StopError};

const LIMIT: Duration = Duration::from_secs(8);
static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tad-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        assert!(path.join("s").as_os_str().len() < 100);
        Self(path)
    }
    fn socket(&self) -> PathBuf {
        self.0.join("s")
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Running {
    shutdown: ShutdownHandle,
    done: mpsc::Receiver<Result<(), DaemonError>>,
    thread: Option<thread::JoinHandle<()>>,
    socket: PathBuf,
    _dir: TempDir,
}
impl Running {
    fn start(sessions: Vec<Session>) -> Self {
        Self::source(Box::new(FixtureSource::new(sessions)), 4096)
    }
    fn source(source: Box<dyn SessionSource>, queue: usize) -> Self {
        Self::source_with_interval(source, queue, Duration::from_millis(30))
    }
    fn source_with_interval(
        source: Box<dyn SessionSource>,
        queue: usize,
        interval: Duration,
    ) -> Self {
        let dir = TempDir::new();
        let socket = dir.socket();
        let mut config = DaemonConfig::new(socket.clone());
        config.poll_interval = interval;
        config.kill_grace = Duration::from_millis(100);
        config.client_queue = queue;
        let daemon = Daemon::bind(config, source).unwrap();
        let shutdown = daemon.shutdown_handle();
        let (tx, done) = mpsc::channel();
        let thread = thread::spawn(move || {
            let _ = tx.send(daemon.run());
        });
        Self {
            shutdown,
            done,
            thread: Some(thread),
            socket,
            _dir: dir,
        }
    }
    fn client(&self) -> Client {
        Client::connect(&self.socket)
    }
    fn finish(&mut self) {
        self.done
            .recv_timeout(LIMIT)
            .expect("daemon shutdown timed out")
            .unwrap();
        self.thread.take().unwrap().join().unwrap();
        assert!(!self.socket.exists(), "shutdown left the socket behind");
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.shutdown.shutdown();
        if self.thread.is_some() && self.done.recv_timeout(LIMIT).is_ok() {
            let _ = self.thread.take().unwrap().join();
        }
    }
}

struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
    events: VecDeque<Event>,
}
impl Client {
    fn connect(path: &Path) -> Self {
        let stream = UnixStream::connect(path).unwrap();
        stream.set_write_timeout(Some(LIMIT)).unwrap();
        Self {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
            next_id: 1,
            events: VecDeque::new(),
        }
    }
    fn send(&mut self, request: Request) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.writer
            .write_all(&encode_line(&ClientMessage { id, request }))
            .unwrap();
        id
    }
    fn receive_until(&mut self, deadline: Instant) -> io::Result<Option<ServerMessage>> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "frame deadline"))?;
        self.reader.get_ref().set_read_timeout(Some(remaining))?;
        let mut line = Vec::new();
        if self.reader.read_until(b'\n', &mut line)? == 0 {
            return Ok(None);
        }
        decode_line(&line).map(Some).map_err(io::Error::other)
    }
    fn receive(&mut self) -> ServerMessage {
        self.receive_until(Instant::now() + LIMIT)
            .unwrap()
            .expect("unexpected EOF")
    }
    fn reply(&mut self, id: u64) -> Result<Response, ErrorCode> {
        let deadline = Instant::now() + LIMIT;
        loop {
            match self
                .receive_until(deadline)
                .unwrap()
                .expect("missing reply")
            {
                ServerMessage::Response {
                    id: actual,
                    response,
                } => {
                    assert_eq!(actual, id, "unexpected or duplicate response");
                    return Ok(response);
                }
                ServerMessage::Error { id: actual, error } => {
                    assert_eq!(actual, id);
                    return Err(error.code);
                }
                ServerMessage::Event { event } => self.events.push_back(event),
            }
        }
    }
    fn request(&mut self, request: Request) -> Result<Response, ErrorCode> {
        let id = self.send(request);
        self.reply(id)
    }
    fn hello(&mut self) {
        assert_eq!(
            self.request(Request::Hello {
                protocol: PROTOCOL_VERSION,
                client: "acceptance-ui".into(),
            }),
            Ok(Response::Welcome {
                protocol: PROTOCOL_VERSION,
                daemon: format!("titan-agentd {}", env!("CARGO_PKG_VERSION")),
            })
        );
    }
    fn done(&mut self, request: Request) {
        assert_eq!(self.request(request), Ok(Response::Done));
    }
    fn event_until(&mut self, deadline: Instant) -> Event {
        self.events.pop_front().unwrap_or_else(|| {
            match self
                .receive_until(deadline)
                .unwrap()
                .expect("missing event")
            {
                ServerMessage::Event { event } => event,
                other => panic!("expected event, got {other:?}"),
            }
        })
    }
    fn output_contains(&mut self, terminal: u64, needle: &[u8]) -> Vec<u8> {
        let deadline = Instant::now() + LIMIT;
        let mut bytes = Vec::new();
        loop {
            if let Event::TerminalOutput {
                terminal: actual,
                data,
            } = self.event_until(deadline)
            {
                assert_eq!(actual, terminal);
                bytes.extend(data.0);
                if contains(&bytes, needle) {
                    return bytes;
                }
            }
        }
    }
    fn open(&mut self, key: SessionKey, cols: u16, rows: u16, reused: bool) -> (u64, Vec<u8>) {
        let id = self.send(Request::OpenTerminal {
            session: key.clone(),
            cols,
            rows,
        });
        // A fresh attachment must receive its response before its snapshot or output.
        let terminal = match self.receive() {
            ServerMessage::Response {
                id: actual,
                response:
                    Response::TerminalOpened {
                        terminal,
                        session,
                        cols: actual_cols,
                        rows: actual_rows,
                        reused: actual_reused,
                    },
            } => {
                assert_eq!(actual, id);
                assert_eq!(session, key);
                assert_eq!(
                    (actual_cols, actual_rows, actual_reused),
                    (cols, rows, reused)
                );
                terminal
            }
            other => panic!("expected TerminalOpened first, got {other:?}"),
        };
        match self.receive() {
            ServerMessage::Event {
                event:
                    Event::TerminalSnapshot {
                        terminal: actual,
                        cols: c,
                        rows: r,
                        data,
                    },
            } => {
                assert_eq!((actual, c, r), (terminal, cols, rows));
                (terminal, data.0)
            }
            other => panic!("expected snapshot second, got {other:?}"),
        }
    }
    fn exited(&mut self, terminal: u64, closing: bool) {
        let deadline = Instant::now() + LIMIT;
        loop {
            if let Event::TerminalExited {
                terminal: actual,
                code,
            } = self.event_until(deadline)
            {
                assert_eq!(actual, terminal);
                if closing {
                    assert_eq!(code, None);
                }
                return;
            }
        }
    }
    fn closed(&mut self) {
        let deadline = Instant::now() + LIMIT;
        loop {
            match self.receive_until(deadline) {
                Ok(None) => return,
                Ok(Some(ServerMessage::Event { .. })) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
                    ) =>
                {
                    return;
                }
                other => panic!("expected closed connection, got {other:?}"),
            }
        }
    }
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|part| part == needle)
}
fn key(id: &str) -> SessionKey {
    SessionKey::new(AgentCli::Codex, id)
}
fn session(id: &str, script: Option<&str>) -> Session {
    Session {
        key: key(id),
        name: format!("fixture-{id}"),
        kind: SessionKind::Interactive,
        state: SessionState::Working,
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        project: Project {
            name: "fixture-project".into(),
            root: std::env::temp_dir().to_string_lossy().into_owned(),
        },
        model: None,
        parent: None,
        started_at_ms: None,
        state_since_ms: Some(-1),
        attach: script.map(|script| vec!["sh".into(), "-c".into(), script.into()]),
    }
}
fn cat_session() -> Session {
    // Disable local echo so every input marker has exactly one output copy.
    session("echo", Some("stty -echo; printf 'ready\\n'; exec cat"))
}
fn subscribe(client: &mut Client) -> Vec<Session> {
    let id = client.send(Request::Subscribe);
    match client.receive() {
        ServerMessage::Response {
            id: actual,
            response: Response::Sessions { sessions },
        } => {
            assert_eq!(actual, id);
            sessions
        }
        other => panic!("Subscribe response must precede session events: {other:?}"),
    }
}
fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[test]
fn greeting_is_required_and_version_mismatch_closes_the_connection() {
    let daemon = Running::start(vec![]);
    for (request, code) in [
        (Request::Subscribe, ErrorCode::NotGreeted),
        (
            Request::Hello {
                protocol: PROTOCOL_VERSION + 1,
                client: "fixture-ui".into(),
            },
            ErrorCode::ProtocolMismatch,
        ),
    ] {
        let mut client = daemon.client();
        assert_eq!(client.request(request), Err(code));
        client.closed();
    }
    let mut client = daemon.client();
    client.hello();
    assert_eq!(
        client.request(Request::Hello {
            protocol: PROTOCOL_VERSION,
            client: "fixture-ui".into()
        }),
        Err(ErrorCode::BadRequest)
    );
    assert!(subscribe(&mut client).is_empty());
}

#[test]
fn malformed_lines_return_id_zero_and_allow_following_requests() {
    let daemon = Running::start(vec![]);
    let mut client = daemon.client();
    client.hello();
    for line in [b"{broken}\n".as_slice(), b"{\"id\":7}\n", b"\xff\n"] {
        client.writer.write_all(line).unwrap();
        assert!(
            matches!(client.receive(), ServerMessage::Error { id: 0, error } if error.code == ErrorCode::BadRequest)
        );
        assert!(subscribe(&mut client).is_empty());
    }
}

#[test]
fn oversized_line_closes_connection() {
    let daemon = Running::start(vec![]);
    let mut client = daemon.client();
    client.hello();
    // The server may close while the oversized frame is still being written.
    let _ = client.writer.write_all(&vec![b'x'; MAX_FRAME_BYTES + 1]);
    let deadline = Instant::now() + LIMIT;
    loop {
        match client.receive_until(deadline) {
            Ok(None) => break,
            Ok(Some(ServerMessage::Error { id: 0, error }))
                if error.code == ErrorCode::BadRequest => {}
            Err(error) if error.kind() == io::ErrorKind::ConnectionReset => break,
            other => panic!("oversized frame did not close connection: {other:?}"),
        }
    }
}

#[test]
fn subscriptions_sort_sessions_and_fixture_stop_updates_state_timestamps() {
    let mut without_timestamp = session("zeta", None);
    without_timestamp.state_since_ms = None;
    let before = unix_ms();
    let mut daemon = Running::source_with_interval(
        Box::new(FixtureSource::new(vec![
            without_timestamp,
            session("alpha", None),
        ])),
        4096,
        Duration::from_secs(60),
    );
    let mut first = daemon.client();
    first.hello();
    let initial = subscribe(&mut first);
    assert_eq!(
        initial.iter().map(|s| s.key.clone()).collect::<Vec<_>>(),
        vec![key("alpha"), key("zeta")]
    );
    assert_eq!(initial[0].state_since_ms, Some(-1));
    assert!((before..=unix_ms()).contains(&initial[1].state_since_ms.unwrap()));
    let mut second = daemon.client();
    second.hello();
    assert_eq!(subscribe(&mut second), initial);
    // Stop must poll immediately, even when the periodic interval is long.
    let before_stop = unix_ms();
    first.done(Request::StopSession {
        session: key("alpha"),
    });
    for client in [&mut first, &mut second] {
        let deadline = Instant::now() + LIMIT;
        let updated = match client.event_until(deadline) {
            Event::Sessions { sessions } => sessions,
            other => panic!("expected changed sessions, got {other:?}"),
        };
        assert_eq!(updated[0].state, SessionState::Finished { failed: false });
        assert!((before_stop..=unix_ms()).contains(&updated[0].state_since_ms.unwrap()));
        assert_eq!(updated[1], initial[1]);
        assert_eq!(subscribe(client), updated);
    }
    daemon.shutdown.shutdown();
    daemon.finish();
}

#[test]
fn terminal_order_echo_sharing_detach_disconnect_and_resize() {
    let daemon = Running::start(vec![cat_session()]);
    let mut first = daemon.client();
    first.hello();
    let (terminal, snapshot) = first.open(key("echo"), 80, 24, false);
    assert_eq!(terminal, 1);
    if !contains(&snapshot, b"ready") {
        first.output_contains(terminal, b"ready");
    }
    let mut second = daemon.client();
    second.hello();
    assert_eq!(second.open(key("echo"), 80, 24, true).0, terminal);
    first.done(Request::Input {
        terminal,
        data: Bytes::from("shared-marker\n"),
    });
    for client in [&mut first, &mut second] {
        let output = client.output_contains(terminal, b"shared-marker");
        assert_eq!(
            output
                .windows(b"shared-marker".len())
                .filter(|p| *p == b"shared-marker")
                .count(),
            1
        );
    }
    first.done(Request::Detach { terminal });
    first.events.clear();
    second.done(Request::Input {
        terminal,
        data: Bytes::from("detached-marker\n"),
    });
    second.output_contains(terminal, b"detached-marker");
    assert!(
        matches!(first.receive_until(Instant::now() + Duration::from_millis(150)), Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock))
    );
    let (same, snapshot) = first.open(key("echo"), 80, 24, true);
    assert_eq!(same, terminal);
    assert!(contains(&snapshot, b"shared-marker"));
    assert!(contains(&snapshot, b"detached-marker"));
    drop(first);
    second.done(Request::Input {
        terminal,
        data: Bytes::from("disconnected-marker\n"),
    });
    second.output_contains(terminal, b"disconnected-marker");
    let mut reopened = daemon.client();
    reopened.hello();
    let (same, snapshot) = reopened.open(key("echo"), 80, 24, true);
    assert_eq!(same, terminal);
    assert!(contains(&snapshot, b"disconnected-marker"));
    second.done(Request::Resize {
        terminal,
        cols: 101,
        rows: 37,
    });
    reopened.done(Request::Resize {
        terminal,
        cols: 93,
        rows: 31,
    });
    let mut observer = daemon.client();
    observer.hello();
    // A reused terminal reports its current size rather than the requested size.
    let id = observer.send(Request::OpenTerminal {
        session: key("echo"),
        cols: 12,
        rows: 7,
    });
    assert_eq!(
        observer.reply(id),
        Ok(Response::TerminalOpened {
            terminal,
            session: key("echo"),
            cols: 93,
            rows: 31,
            reused: true
        })
    );
    assert!(matches!(
        observer.event_until(Instant::now() + LIMIT),
        Event::TerminalSnapshot {
            cols: 93,
            rows: 31,
            ..
        }
    ));
    let close_id = second.send(Request::CloseTerminal { terminal });
    assert_eq!(second.reply(close_id), Ok(Response::Done));
    assert!(
        second.events.iter().any(|event| matches!(event,
            Event::TerminalExited { terminal: actual, code: None } if *actual == terminal
        )),
        "CloseTerminal replied before publishing TerminalExited"
    );
    for client in [&mut second, &mut reopened, &mut observer] {
        client.exited(terminal, true);
    }
    assert_eq!(
        second.request(Request::Input {
            terminal,
            data: Bytes::from("late\n")
        }),
        Err(ErrorCode::UnknownTerminal)
    );
    assert_eq!(
        second.request(Request::Resize {
            terminal,
            cols: 80,
            rows: 24
        }),
        Err(ErrorCode::UnknownTerminal)
    );
    let mut fresh = daemon.client();
    fresh.hello();
    assert_eq!(fresh.open(key("echo"), 80, 24, false).0, terminal + 1);
}

#[test]
fn snapshot_and_following_output_reconstruct_every_line_once_during_attachment() {
    let daemon = Running::start(vec![session(
        "sequence",
        Some(
            "stty -echo; printf 'ready\\n'; read line; i=0; while [ \"$i\" -lt 500 ]; do printf 'item-%03d\\n' \"$i\"; i=$((i+1)); done; printf 'COMPLETE\\n'; exec cat",
        ),
    )]);
    let mut producer = daemon.client();
    producer.hello();
    let (terminal, snapshot) = producer.open(key("sequence"), 80, 600, false);
    if !contains(&snapshot, b"ready") {
        producer.output_contains(terminal, b"ready\r\n");
    }
    producer.done(Request::Input {
        terminal,
        data: Bytes::from("begin\n"),
    });
    let mut observer = daemon.client();
    observer.hello();
    let (same, snapshot) = observer.open(key("sequence"), 80, 600, true);
    assert_eq!(same, terminal);
    // Replay exactly what a newly attached UI receives. All lines fit on the
    // screen, so missing or duplicated bytes change the resulting screen.
    let mut replay = Emulator::new(80, 600, 1000);
    let _ = replay.feed(&snapshot);
    let deadline = Instant::now() + LIMIT;
    while !contains(&replay.snapshot(), b"COMPLETE") {
        match observer.event_until(deadline) {
            Event::TerminalOutput {
                terminal: actual,
                data,
            } => {
                assert_eq!(actual, terminal);
                let _ = replay.feed(&data.0);
            }
            other => panic!("expected output following snapshot, got {other:?}"),
        }
    }
    // A second input is a barrier after the complete producer sequence.
    observer.done(Request::Input {
        terminal,
        data: Bytes::from("BARRIER\n"),
    });
    let deadline = Instant::now() + LIMIT;
    let mut barrier_output = Vec::new();
    while !contains(&barrier_output, b"BARRIER\r\n") {
        match observer.event_until(deadline) {
            Event::TerminalOutput {
                terminal: actual,
                data,
            } => {
                assert_eq!(actual, terminal);
                let _ = replay.feed(&data.0);
                barrier_output.extend(data.0);
            }
            other => panic!("expected barrier output, got {other:?}"),
        }
    }
    let mut expected = Emulator::new(80, 600, 1000);
    let _ = expected.feed(b"ready\r\n");
    for index in 0..500 {
        let _ = expected.feed(format!("item-{index:03}\r\n").as_bytes());
    }
    let _ = expected.feed(b"COMPLETE\r\nBARRIER\r\n");
    assert_eq!(replay.snapshot(), expected.snapshot());
}

#[test]
fn resize_reaches_the_pty_and_existing_session_directory_is_used() {
    let dir = TempDir::new();
    let mut fixture = session(
        "size",
        Some("stty -echo; printf 'ready\\n'; while IFS= read -r line; do stty size; pwd; done"),
    );
    fixture.cwd = dir.0.to_string_lossy().into_owned();
    let daemon = Running::start(vec![fixture]);
    let mut client = daemon.client();
    client.hello();
    let (terminal, snapshot) = client.open(key("size"), 80, 24, false);
    if !contains(&snapshot, b"ready") {
        client.output_contains(terminal, b"ready");
    }
    client.done(Request::Resize {
        terminal,
        cols: 111,
        rows: 39,
    });
    client.done(Request::Input {
        terminal,
        data: Bytes::from("measure\n"),
    });
    let mut output = client.output_contains(terminal, b"39 111");
    let path = dir.0.to_str().unwrap().as_bytes();
    if !contains(&output, path) {
        output.extend(client.output_contains(terminal, path));
    }
    assert!(contains(&output, path));
}

#[test]
fn natural_exit_can_be_reopened_with_a_new_monotonic_id() {
    let daemon = Running::start(vec![session(
        "short",
        Some("stty -echo; printf 'ready\\n'; read line; exit 0"),
    )]);
    let mut client = daemon.client();
    client.hello();
    let (first, snapshot) = client.open(key("short"), 80, 24, false);
    assert_eq!(first, 1);
    if !contains(&snapshot, b"ready") {
        client.output_contains(first, b"ready");
    }
    client.done(Request::Input {
        terminal: first,
        data: Bytes::from("finish\n"),
    });
    client.exited(first, false);
    assert_eq!(
        client.request(Request::Input {
            terminal: first,
            data: Bytes::from("late\n")
        }),
        Err(ErrorCode::UnknownTerminal)
    );
    let (second, _) = client.open(key("short"), 80, 24, false);
    assert_eq!(second, first + 1);
}

#[test]
fn missing_sessions_unattachable_sessions_failed_spawn_and_missing_terminals() {
    let mut broken = session("broken", None);
    broken.attach = Some(vec!["/invented-no-such-titan-program".into()]);
    let daemon = Running::start(vec![session("plain", None), broken]);
    let mut client = daemon.client();
    client.hello();
    for (name, code) in [
        ("missing", ErrorCode::UnknownSession),
        ("plain", ErrorCode::NotAttachable),
        ("broken", ErrorCode::SpawnFailed),
    ] {
        assert_eq!(
            client.request(Request::OpenTerminal {
                session: key(name),
                cols: 80,
                rows: 24
            }),
            Err(code)
        );
    }
    assert_eq!(
        client.request(Request::StopSession {
            session: key("missing")
        }),
        Err(ErrorCode::UnknownSession)
    );
    for request in [
        Request::Input {
            terminal: 999,
            data: Bytes::from("test"),
        },
        Request::Resize {
            terminal: 999,
            cols: 80,
            rows: 24,
        },
    ] {
        assert_eq!(client.request(request), Err(ErrorCode::UnknownTerminal));
    }
}

struct FailingStop {
    failed: bool,
}
impl SessionSource for FailingStop {
    fn poll(&mut self) -> Vec<Session> {
        vec![session("stop", None)]
    }
    fn stop(&mut self, _: &Session) -> Result<(), StopError> {
        if self.failed {
            Err(StopError::Failed("fixture stop failure".into()))
        } else {
            Err(StopError::Unsupported("fixture stop unsupported".into()))
        }
    }
}
#[test]
fn stop_errors_are_mapped_to_protocol_errors() {
    for (failed, code) in [
        (false, ErrorCode::Unsupported),
        (true, ErrorCode::StopFailed),
    ] {
        let daemon = Running::source(Box::new(FailingStop { failed }), 4096);
        let mut client = daemon.client();
        client.hello();
        assert_eq!(
            client.request(Request::StopSession {
                session: key("stop")
            }),
            Err(code)
        );
    }
}

#[test]
fn slow_client_is_disconnected_without_blocking_other_terminals_or_shutdown() {
    let mut daemon = Running::source(
        Box::new(FixtureSource::new(vec![
            session("flood", Some("stty -echo; read line; exec yes flood")),
            cat_session(),
        ])),
        32,
    );
    let mut slow = daemon.client();
    slow.hello();
    let (flood, _) = slow.open(key("flood"), 80, 24, false);
    // Stop reading before starting an unbounded output producer.
    slow.send(Request::Input {
        terminal: flood,
        data: Bytes::from("start\n"),
    });
    let mut healthy = daemon.client();
    healthy.hello();
    let (terminal, snapshot) = healthy.open(key("echo"), 80, 24, false);
    if !contains(&snapshot, b"ready") {
        healthy.output_contains(terminal, b"ready");
    }
    healthy.done(Request::Input {
        terminal,
        data: Bytes::from("healthy-marker\n"),
    });
    healthy.output_contains(terminal, b"healthy-marker");
    // Give the socket buffer time to fill, without an unbounded sleep.
    let (_tx, rx) = mpsc::channel::<()>();
    assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
    let deadline = Instant::now() + LIMIT;
    loop {
        match slow.receive_until(deadline) {
            Ok(None) => break,
            Ok(Some(_)) => {}
            Err(error) if error.kind() == io::ErrorKind::ConnectionReset => break,
            other => panic!("slow client was not disconnected: {other:?}"),
        }
    }
    healthy.done(Request::Input {
        terminal,
        data: Bytes::from("still-alive\n"),
    });
    healthy.output_contains(terminal, b"still-alive");
    let _idle = daemon.client();
    daemon.shutdown.shutdown();
    daemon.shutdown.shutdown();
    daemon.finish();
}

#[test]
fn shutdown_request_replies_then_removes_socket_and_releases_lock() {
    let mut daemon = Running::start(vec![cat_session()]);
    let mut client = daemon.client();
    client.hello();
    client.open(key("echo"), 80, 24, false);
    client.done(Request::Shutdown);
    daemon.finish();
    client.closed();
    let rebound = Daemon::bind(
        DaemonConfig::new(daemon.socket.clone()),
        Box::new(EmptySource),
    )
    .unwrap();
    let shutdown = rebound.shutdown_handle();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _ = tx.send(rebound.run());
    });
    shutdown.shutdown();
    rx.recv_timeout(LIMIT).unwrap().unwrap();
    worker.join().unwrap();
}

#[test]
fn bind_rejects_second_daemon_and_replaces_orphaned_socket() {
    let daemon = Running::start(vec![]);
    assert!(matches!(
        Daemon::bind(
            DaemonConfig::new(daemon.socket.clone()),
            Box::new(EmptySource)
        ),
        Err(DaemonError::AlreadyRunning)
    ));
    let dir = TempDir::new();
    let socket = dir.socket();
    drop(UnixListener::bind(&socket).unwrap());
    let bound = Daemon::bind(DaemonConfig::new(socket.clone()), Box::new(EmptySource)).unwrap();
    let shutdown = bound.shutdown_handle();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _ = tx.send(bound.run());
    });
    let mut client = Client::connect(&socket);
    client.hello();
    shutdown.shutdown();
    rx.recv_timeout(LIMIT).unwrap().unwrap();
    worker.join().unwrap();
    assert!(!socket.exists());
}

#[test]
fn config_defaults_and_new_socket_directory_permissions() {
    fn assert_error<T: std::error::Error>() {}
    assert_error::<DaemonError>();
    let dir = TempDir::new();
    let folder = dir.0.join("private");
    let config = DaemonConfig::new(folder.join("s"));
    assert_eq!(config.poll_interval, Duration::from_secs(3));
    assert_eq!(config.client_queue, 4096);
    assert_eq!(config.kill_grace, Duration::from_secs(2));
    let daemon = Daemon::bind(config, Box::new(EmptySource)).unwrap();
    assert_eq!(
        fs::metadata(folder).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let shutdown = daemon.shutdown_handle();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _ = tx.send(daemon.run());
    });
    shutdown.shutdown();
    rx.recv_timeout(LIMIT).unwrap().unwrap();
    worker.join().unwrap();
}

#[test]
fn fixture_file_and_empty_source_use_the_public_source_contract() {
    let dir = TempDir::new();
    let path = dir.0.join("sessions.json");
    let expected = vec![session("fixture", None)];
    fs::write(&path, serde_json::to_vec(&expected).unwrap()).unwrap();
    let mut fixture = FixtureSource::from_file(&path).unwrap();
    assert_eq!(fixture.poll(), expected);
    fixture.stop(&expected[0]).unwrap();
    assert_eq!(
        fixture.poll()[0].state,
        SessionState::Finished { failed: false }
    );
    fs::write(&path, b"invalid JSON").unwrap();
    assert!(FixtureSource::from_file(&path).is_err());
    assert!(FixtureSource::from_file(&dir.0.join("missing.json")).is_err());
    let mut empty = EmptySource;
    assert!(empty.poll().is_empty());
    assert!(matches!(
        empty.stop(&expected[0]),
        Err(StopError::Unsupported(_))
    ));
}

struct CountingSource {
    polls: Arc<AtomicUsize>,
}
impl SessionSource for CountingSource {
    fn poll(&mut self) -> Vec<Session> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Vec::new()
    }
}

#[test]
fn discovery_stops_after_last_subscriber_disconnects_and_resumes_on_subscribe() {
    let interval = Duration::from_millis(40);
    let polls = Arc::new(AtomicUsize::new(0));
    let daemon = Running::source_with_interval(
        Box::new(CountingSource {
            polls: polls.clone(),
        }),
        4096,
        interval,
    );
    // Keep a greeted, unsubscribed client connected throughout the idle period.
    let mut observer = daemon.client();
    observer.hello();
    for _ in 0..2 {
        let mut subscriber = daemon.client();
        subscriber.hello();
        assert!(subscribe(&mut subscriber).is_empty());
        let before = polls.load(Ordering::SeqCst);
        let deadline = Instant::now() + LIMIT;
        while polls.load(Ordering::SeqCst) < before + 2 {
            assert!(
                Instant::now() < deadline,
                "subscribed discovery did not advance"
            );
            thread::sleep(interval);
        }
        drop(subscriber);
        // Allow disconnect processing and any poll already in flight to finish.
        thread::sleep(interval * 2);
        let idle = polls.load(Ordering::SeqCst);
        for _ in 0..5 {
            thread::sleep(interval);
            assert_eq!(
                polls.load(Ordering::SeqCst),
                idle,
                "discovery continued without subscribers"
            );
        }
    }
}

struct ControlledSource {
    sessions: Arc<Mutex<Vec<Session>>>,
    polls: mpsc::Sender<()>,
}
impl SessionSource for ControlledSource {
    fn poll(&mut self) -> Vec<Session> {
        let result = self.sessions.lock().unwrap().clone();
        let _ = self.polls.send(());
        result
    }
}
#[test]
fn discovery_polls_when_subscribed_and_preserves_timestamp_until_state_changes() {
    let state = Arc::new(Mutex::new(vec![session("mutable", None)]));
    let (tx, rx) = mpsc::channel();
    let daemon = Running::source(
        Box::new(ControlledSource {
            sessions: state.clone(),
            polls: tx,
        }),
        4096,
    );
    rx.recv_timeout(LIMIT).unwrap();
    assert!(
        rx.recv_timeout(Duration::from_millis(150)).is_err(),
        "discovery polled without subscribers"
    );
    let mut client = daemon.client();
    client.hello();
    let initial = subscribe(&mut client);
    assert_eq!(initial[0].state_since_ms, Some(-1));
    rx.recv_timeout(LIMIT).unwrap();
    assert_eq!(subscribe(&mut client), initial);
    assert!(
        matches!(
            client.receive_until(Instant::now() + Duration::from_millis(150)),
            Err(error) if matches!(error.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock)
        ),
        "unchanged sessions generated an event"
    );
    assert_eq!(
        client.request(Request::StopSession {
            session: key("mutable")
        }),
        Err(ErrorCode::Unsupported)
    );
    {
        let mut sessions = state.lock().unwrap();
        sessions[0].name = "renamed-fixture".into();
        sessions[0].state_since_ms = Some(-2);
    }
    let changed = match client.event_until(Instant::now() + LIMIT) {
        Event::Sessions { sessions } => sessions,
        other => panic!("expected session update: {other:?}"),
    };
    assert_eq!(changed[0].name, "renamed-fixture");
    assert_eq!(changed[0].state_since_ms, initial[0].state_since_ms);
    let before_state_change = unix_ms();
    state.lock().unwrap()[0].state = SessionState::Waiting;
    let changed = match client.event_until(Instant::now() + LIMIT) {
        Event::Sessions { sessions } => sessions,
        other => panic!("expected state update: {other:?}"),
    };
    assert_eq!(changed[0].state, SessionState::Waiting);
    assert!((before_state_change..=unix_ms()).contains(&changed[0].state_since_ms.unwrap()));
    state.lock().unwrap().clear();
    assert!(matches!(
        client.event_until(Instant::now() + LIMIT),
        Event::Sessions { sessions } if sessions.is_empty()
    ));
    assert_eq!(
        client.request(Request::OpenTerminal {
            session: key("mutable"),
            cols: 80,
            rows: 24
        }),
        Err(ErrorCode::UnknownSession)
    );
}

struct ChildGuard(std::process::Child);
impl ChildGuard {
    fn wait(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + LIMIT;
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "binary did not exit before deadline"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn stderr(&mut self) -> String {
        let mut text = String::new();
        self.0
            .stderr
            .as_mut()
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        text
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn binary(dir: &TempDir) -> std::process::Command {
    let bin = dir.0.join("bin");
    let home = dir.0.join("home");
    let codex = dir.0.join("codex");
    let claude = dir.0.join("claude");
    for folder in [&bin, &home, &codex, &claude] {
        fs::create_dir_all(folder).unwrap();
        assert_eq!(fs::read_dir(folder).unwrap().count(), 0);
    }
    assert!(!bin.join("claude").exists(), "claude must not be reachable");
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_titan-agentd"));
    command
        .env_clear()
        .env("PATH", &bin)
        .env("HOME", &home)
        .env("CODEX_HOME", &codex)
        .env("CLAUDE_CONFIG_DIR", &claude)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    command
}
fn wait_for_binary_socket(child: &mut ChildGuard, socket: &Path) -> Client {
    let deadline = Instant::now() + LIMIT;
    loop {
        if let Ok(stream) = UnixStream::connect(socket) {
            drop(stream);
            return Client::connect(socket);
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "daemon binary exited before binding"
        );
        assert!(
            Instant::now() < deadline,
            "binary did not bind before deadline"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn binary_reports_argument_errors_with_usage_and_exit_code_two() {
    for args in [
        vec!["--invented-option"],
        vec!["--socket"],
        vec!["--fixture"],
        vec!["unexpected-argument"],
    ] {
        let dir = TempDir::new();
        let mut child = ChildGuard(binary(&dir).args(args).spawn().unwrap());
        assert_eq!(child.wait().code(), Some(2));
        let stderr = child.stderr();
        assert!(
            stderr.contains("--socket") && stderr.contains("--fixture"),
            "missing usage: {stderr}"
        );
    }
}

#[test]
fn binary_reads_fixture_honors_explicit_socket_and_exits_cleanly_if_already_running() {
    let dir = TempDir::new();
    let socket = dir.socket();
    let fixture = dir.0.join("fixture.json");
    fs::write(
        &fixture,
        serde_json::to_vec(&vec![session("binary-fixture", None)]).unwrap(),
    )
    .unwrap();
    let ignored_socket = dir.0.join("ignored");
    let mut child = ChildGuard(
        binary(&dir)
            .arg("--socket")
            .arg(&socket)
            .arg("--fixture")
            .arg(&fixture)
            .env(titan_agent_core::paths::SOCKET_ENV, &ignored_socket)
            .spawn()
            .unwrap(),
    );
    let mut client = wait_for_binary_socket(&mut child, &socket);
    client.hello();
    let sessions = subscribe(&mut client);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].key, key("binary-fixture"));
    assert!(!ignored_socket.exists());
    let mut second = ChildGuard(binary(&dir).arg("--socket").arg(&socket).spawn().unwrap());
    assert_eq!(second.wait().code(), Some(0));
    assert!(!second.stderr().trim().is_empty());
    client.done(Request::Shutdown);
    assert!(child.wait().success());
    assert!(!socket.exists());
}

#[test]
fn binary_uses_socket_environment_and_handles_termination_signals() {
    for signal in ["-TERM", "-INT"] {
        let dir = TempDir::new();
        let socket = dir.socket();
        let mut child = ChildGuard(
            binary(&dir)
                .env(titan_agent_core::paths::SOCKET_ENV, &socket)
                .spawn()
                .unwrap(),
        );
        let mut client = wait_for_binary_socket(&mut child, &socket);
        client.hello();
        assert!(subscribe(&mut client).is_empty());
        let mut sender = ChildGuard(
            std::process::Command::new("kill")
                .arg(signal)
                .arg(child.0.id().to_string())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        );
        assert!(sender.wait().success());
        assert!(child.wait().success());
        assert!(!socket.exists());
        client.closed();
    }
}

#[cfg(target_os = "linux")]
#[test]
fn shutdown_reaps_all_terminal_processes_with_connected_idle_clients() {
    let dir = TempDir::new();
    let mut first = session(
        "first-process",
        Some("printf '%s' \"$$\" > first.pid; printf 'ready\\n'; exec cat"),
    );
    let mut second = session(
        "second-process",
        Some("printf '%s' \"$$\" > second.pid; printf 'ready\\n'; exec cat"),
    );
    first.cwd = dir.0.to_string_lossy().into_owned();
    second.cwd = dir.0.to_string_lossy().into_owned();
    let mut daemon = Running::start(vec![first, second]);
    let mut clients = Vec::new();
    let mut pids = Vec::new();
    for (name, file) in [
        ("first-process", "first.pid"),
        ("second-process", "second.pid"),
    ] {
        let mut client = daemon.client();
        client.hello();
        let (terminal, snapshot) = client.open(key(name), 80, 24, false);
        if !contains(&snapshot, b"ready") {
            client.output_contains(terminal, b"ready");
        }
        let pid: u32 = fs::read_to_string(dir.0.join(file))
            .unwrap()
            .parse()
            .unwrap();
        assert!(Path::new(&format!("/proc/{pid}")).exists());
        pids.push(pid);
        clients.push(client);
    }
    daemon.shutdown.shutdown();
    daemon.finish();
    for pid in pids {
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "terminal process survived shutdown"
        );
    }
    for client in &mut clients {
        client.closed();
    }
}

#[test]
fn bind_returns_io_error_when_socket_parent_is_a_file() {
    let dir = TempDir::new();
    let parent = dir.0.join("ordinary-file");
    fs::write(&parent, b"fixture").unwrap();
    assert!(matches!(
        Daemon::bind(DaemonConfig::new(parent.join("s")), Box::new(EmptySource)),
        Err(DaemonError::Io(_))
    ));
}

#[test]
fn concurrent_opens_share_one_terminal_and_spawn_only_one_process() {
    let dir = TempDir::new();
    let mut fixture = session(
        "concurrent-open",
        Some("echo started >> started; printf 'ready\\n'; exec cat"),
    );
    fixture.cwd = dir.0.to_string_lossy().into_owned();
    let daemon = Running::start(vec![fixture]);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let (tx, rx) = mpsc::channel();
    let mut workers = Vec::new();
    for _ in 0..2 {
        let mut client = daemon.client();
        client.hello();
        let barrier = barrier.clone();
        let tx = tx.clone();
        workers.push(thread::spawn(move || {
            barrier.wait();
            let (terminal, reused) = match client
                .request(Request::OpenTerminal {
                    session: key("concurrent-open"),
                    cols: 80,
                    rows: 24,
                })
                .unwrap()
            {
                Response::TerminalOpened {
                    terminal, reused, ..
                } => (terminal, reused),
                other => panic!("expected TerminalOpened, got {other:?}"),
            };
            let snapshot = match client.event_until(Instant::now() + LIMIT) {
                Event::TerminalSnapshot {
                    terminal: actual,
                    data,
                    ..
                } => {
                    assert_eq!(actual, terminal);
                    data.0
                }
                other => panic!("expected snapshot, got {other:?}"),
            };
            // The marker is written before ready, so both spawns (if any) have
            // reached the marker before the parent inspects the file.
            if !contains(&snapshot, b"ready") {
                client.output_contains(terminal, b"ready");
            }
            tx.send((terminal, reused, client)).unwrap();
        }));
    }
    drop(tx);
    let deadline = Instant::now() + LIMIT;
    let first = rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .unwrap();
    let second = rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .unwrap();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(first.0, second.0);
    assert_ne!(
        first.1, second.1,
        "exactly one open must create the terminal"
    );
    assert_eq!(
        fs::read_to_string(dir.0.join("started")).unwrap(),
        "started\n"
    );
}

#[test]
fn terminal_answers_initial_cursor_query_before_program_prints_ok() {
    let daemon = Running::start(vec![session(
        "initial-cursor-query",
        Some(
            "stty raw -echo; printf '\\033[6n'; dd bs=1 count=6 2>/dev/null >/dev/null; printf ok; exec cat",
        ),
    )]);
    let mut client = daemon.client();
    client.hello();
    let (terminal, snapshot) = client.open(key("initial-cursor-query"), 80, 24, false);
    if !contains(&snapshot, b"ok") {
        client.output_contains(terminal, b"ok");
    }
}

#[test]
fn close_terminal_completes_while_program_continuously_queries_cursor_position() {
    let daemon = Running::start(vec![session(
        "cursor-queries",
        Some("while :; do printf '\\033[6n'; sleep 0.01; done"),
    )]);
    let mut client = daemon.client();
    client.hello();
    let (terminal, _) = client.open(key("cursor-queries"), 80, 24, false);
    client.output_contains(terminal, b"\x1b[6n");
    let started = Instant::now();
    let deadline = started + Duration::from_secs(5);
    let id = client.send(Request::CloseTerminal { terminal });
    loop {
        match client
            .receive_until(deadline)
            .unwrap()
            .expect("missing close reply")
        {
            ServerMessage::Event { .. } => {}
            ServerMessage::Response {
                id: actual,
                response,
            } => {
                assert_eq!(actual, id);
                assert_eq!(response, Response::Done);
                break;
            }
            other => panic!("expected close reply, got {other:?}"),
        }
    }
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn shutdown_sender_receives_done_before_connection_closes() {
    let mut daemon = Running::start(vec![]);
    let mut client = daemon.client();
    client.hello();
    let id = client.send(Request::Shutdown);
    assert_eq!(
        client.receive_until(Instant::now() + LIMIT).unwrap(),
        Some(ServerMessage::Response {
            id,
            response: Response::Done,
        })
    );
    client.closed();
    daemon.finish();
}

#[test]
fn bind_preserves_existing_directory_permissions_and_creates_private_directory() {
    let dir = TempDir::new();
    for (name, existing, expected) in [("existing", true, 0o755), ("new", false, 0o700)] {
        let folder = dir.0.join(name);
        if existing {
            fs::create_dir(&folder).unwrap();
            fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        } else {
            assert!(!folder.exists());
        }
        let daemon =
            Daemon::bind(DaemonConfig::new(folder.join("s")), Box::new(EmptySource)).unwrap();
        let mode = fs::metadata(&folder).unwrap().permissions().mode() & 0o777;
        let shutdown = daemon.shutdown_handle();
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let _ = tx.send(daemon.run());
        });
        shutdown.shutdown();
        rx.recv_timeout(LIMIT).unwrap().unwrap();
        worker.join().unwrap();
        assert_eq!(
            mode, expected,
            "unexpected permissions for {name} directory"
        );
    }
}
