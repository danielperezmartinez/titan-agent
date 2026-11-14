#![cfg(target_os = "linux")]

use alacritty_terminal::{
    event::VoidListener,
    index::{Column, Line, Point},
    term::{Term, test::TermSize},
    vte::ansi::{Processor, StdSyncHandler},
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs,
    io::{self, BufRead, BufReader, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
use titan_agent_core::protocol::{
    ClientRequest as Request, DaemonNotification as Notification, MAX_LINE_BYTES, Message,
    PeerInfo, SessionsResult, decode_line, encode_line,
};
use titan_agentd::{
    config::Config,
    server::{Daemon, DaemonHandle},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const WAIT: Duration = Duration::from_secs(5);

struct Fixture {
    root: PathBuf,
    daemon: Option<DaemonHandle>,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "titan-accept-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("agents.json"), "[]").unwrap();
        let script = root.join("fake-claude");
        fs::write(
            &script,
            r##"#!/bin/sh
base=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
case "$1" in
  agents)
    [ "$2" = --json ] || exit 42
    [ ! -f "$base/fail-discovery" ] || exit 43
    cat "$base/agents.json" ;;
  attach)
    [ "$TERM" = xterm-256color ] || exit 44
    [ "$COLORTERM" = truecolor ] || exit 45
    printf '%s\n' "$2" >> "$base/attached"
    printf 'attached:%s\n' "$2"
    exec cat ;;
  stop)
    if [ "$2" = fail-stop ]; then printf 'fixture stop failure\n' >&2; exit 46; fi
    printf '%s\n' "$2" >> "$base/stopped" ;;
  *) exit 47 ;;
esac
"##,
        )
        .unwrap();
        fs::set_permissions(script, fs::Permissions::from_mode(0o700)).unwrap();
        Self { root, daemon: None }
    }
    fn config(&self) -> Config {
        let mut config = Config::new(self.root.join("socket-dir/agentd.sock"));
        config.claude_program = self.root.join("fake-claude").to_str().unwrap().into();
        config.codex_program = None;
        config.git_program = None;
        config.poll_subscribed = Duration::from_millis(40);
        config.poll_idle = Duration::from_millis(80);
        config
    }
    fn sessions(&self, extra: bool) {
        let cwd = self.root.to_str().unwrap();
        let mut agents = vec![
            json!({"id":"alpha","name":"Alpha fixture","cwd":cwd,"pid":12345,"state":"working"}),
            json!({"sessionId":"archived","cwd":cwd,"state":"stopped"}),
            json!({"id":"fail-stop","cwd":cwd,"pid":12346}),
        ];
        if extra {
            agents.push(json!({"id":"beta","cwd":cwd}));
        }
        fs::write(
            self.root.join("agents.next"),
            serde_json::to_vec(&agents).unwrap(),
        )
        .unwrap();
        fs::rename(self.root.join("agents.next"), self.root.join("agents.json")).unwrap();
    }
    fn start(&mut self) {
        self.daemon = Some(Daemon::start(self.config()).unwrap());
        assert_eq!(self.daemon.as_ref().unwrap().socket(), self.config().socket);
    }
    fn connect(&self) -> Client {
        Client::new(UnixStream::connect(self.config().socket).unwrap())
    }
    fn ready(&self) -> Client {
        let mut client = self.connect();
        client.hello();
        let deadline = Instant::now() + WAIT;
        loop {
            let sessions: SessionsResult =
                serde_json::from_value(client.ok(Request::SessionsList)).unwrap();
            if sessions.sessions.len() == 3 {
                break;
            }
            assert!(Instant::now() < deadline, "initial discovery timed out");
            thread::sleep(Duration::from_millis(20));
        }
        client
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(daemon) = self.daemon.take() {
            daemon.shutdown();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    id: u64,
    pending: VecDeque<Message>,
}
impl Client {
    fn new(stream: UnixStream) -> Self {
        stream.set_read_timeout(Some(WAIT)).unwrap();
        stream.set_write_timeout(Some(WAIT)).unwrap();
        Self {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
            id: 0,
            pending: VecDeque::new(),
        }
    }
    fn send(&mut self, request: Request) -> u64 {
        self.id += 1;
        self.writer
            .write_all(
                encode_line(&Message::Request {
                    id: self.id,
                    request,
                })
                .as_bytes(),
            )
            .unwrap();
        self.id
    }
    fn next(&mut self) -> Message {
        self.pending.pop_front().unwrap_or_else(|| self.read())
    }
    fn read(&mut self) -> Message {
        let mut line = String::new();
        assert!(
            self.reader.read_line(&mut line).unwrap() > 0,
            "unexpected EOF"
        );
        decode_line(&line).unwrap()
    }
    fn response(&mut self, id: u64) -> Result<Value, titan_agent_core::protocol::RpcError> {
        let deadline = Instant::now() + WAIT;
        loop {
            match self.read() {
                Message::Response { id: actual, result } => {
                    assert_eq!(actual, id);
                    return result;
                }
                notification => self.pending.push_back(notification),
            }
            assert!(Instant::now() < deadline, "response timed out");
        }
    }
    fn ok(&mut self, request: Request) -> Value {
        let id = self.send(request);
        self.response(id).unwrap()
    }
    fn error(&mut self, request: Request, code: i32) {
        let id = self.send(request);
        assert_eq!(self.response(id).unwrap_err().code, code);
    }
    fn hello(&mut self) {
        let result = self.ok(hello(1));
        assert_eq!(
            result,
            json!({"protocol":1,"daemon":{"name":"titan-agentd","version":env!("CARGO_PKG_VERSION")}})
        );
    }
    fn closed(&mut self) {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => (),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
                ) => {}
            other => panic!("connection should close: {other:?}"),
        }
    }
    fn output_until(&mut self, term: &mut Term<VoidListener>, needle: &str) {
        let deadline = Instant::now() + WAIT;
        while !screen(term).contains(needle) {
            match self.next() {
                Message::Notification(Notification::TerminalOutput { session, data }) => {
                    assert_eq!(session, "claude:alpha");
                    Processor::<StdSyncHandler>::new().advance(term, &data);
                }
                other => panic!("expected terminal output: {other:?}"),
            }
            assert!(Instant::now() < deadline, "output missing {needle:?}");
        }
    }
}
fn hello(protocol: u32) -> Request {
    Request::Hello {
        protocol,
        client: PeerInfo {
            name: "fixture-client".into(),
            version: "0".into(),
        },
    }
}
fn open(session: &str, cols: u16, rows: u16) -> Request {
    Request::TerminalOpen {
        session: session.into(),
        cols,
        rows,
    }
}
fn term() -> Term<VoidListener> {
    Term::new(Default::default(), &TermSize::new(80, 24), VoidListener)
}
fn screen(term: &Term<VoidListener>) -> String {
    (0..24)
        .map(|row| {
            (0..80)
                .map(|col| term.grid()[Point::new(Line(row), Column(col))].c)
                .collect::<String>()
                + "\n"
        })
        .collect()
}

#[test]
fn handshake_errors_and_socket_ownership_and_shutdown() {
    let mut fixture = Fixture::new();
    fixture.start();
    assert_eq!(
        fs::metadata(fixture.root.join("socket-dir"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let error = Daemon::start(fixture.config())
        .err()
        .expect("second daemon must fail");
    assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
    let mut client = fixture.connect();
    client.error(Request::SessionsList, 2);
    client.hello();
    let mut incompatible = fixture.connect();
    incompatible.error(hello(2), 1);
    incompatible.closed();
    // A rejected client must not affect an established client.
    client.ok(Request::SessionsList);
    fixture.daemon.take().unwrap().shutdown();
    client.closed();
    assert!(!fixture.config().socket.exists());
}

#[test]
fn stale_socket_is_replaced_and_existing_directory_permissions_are_kept() {
    let mut fixture = Fixture::new();
    let config = fixture.config();
    fs::create_dir(config.socket.parent().unwrap()).unwrap();
    fs::set_permissions(
        config.socket.parent().unwrap(),
        fs::Permissions::from_mode(0o750),
    )
    .unwrap();
    drop(UnixListener::bind(&config.socket).unwrap());
    fixture.start();
    fixture.connect().hello();
    assert_eq!(
        fs::metadata(config.socket.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o750
    );
}

#[test]
fn discovery_lists_subscribes_and_preserves_previous_claude_list_on_failure() {
    let mut fixture = Fixture::new();
    fixture.sessions(false);
    fixture.start();
    let mut client = fixture.ready();
    let initial: SessionsResult =
        serde_json::from_value(client.ok(Request::SessionsSubscribe)).unwrap();
    assert_eq!(
        initial
            .sessions
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["claude:alpha", "claude:archived", "claude:fail-stop"]
    );
    assert_eq!(
        initial.sessions[0].project.root,
        fixture.root.to_str().unwrap()
    );
    fixture.sessions(true);
    match client.next() {
        Message::Notification(Notification::SessionsChanged { sessions }) => {
            assert_eq!(sessions.len(), 4);
            assert_eq!(sessions[3].id, "claude:beta");
        }
        other => panic!("expected changed notification: {other:?}"),
    }
    // A request acts as a barrier: unchanged polls must not queue notifications.
    thread::sleep(Duration::from_millis(180));
    let id = client.send(Request::SessionsList);
    assert!(matches!(client.next(), Message::Response { id: actual, .. } if actual == id));
    fs::write(fixture.root.join("agents.json"), "invalid-json").unwrap();
    thread::sleep(Duration::from_millis(180));
    let result: SessionsResult = serde_json::from_value(client.ok(Request::SessionsList)).unwrap();
    assert_eq!(result.sessions.len(), 4);
    fs::write(fixture.root.join("fail-discovery"), "").unwrap();
    thread::sleep(Duration::from_millis(180));
    let result: SessionsResult = serde_json::from_value(client.ok(Request::SessionsList)).unwrap();
    assert_eq!(result.sessions.len(), 4);
}

#[test]
fn terminal_survives_disconnect_shares_one_child_and_reopens_after_exit() {
    let mut fixture = Fixture::new();
    fixture.sessions(false);
    fixture.start();
    let mut first = fixture.ready();
    let id = first.send(open("claude:alpha", 80, 24));
    assert_eq!(
        first.next(),
        Message::Response {
            id,
            result: Ok(json!({}))
        }
    );
    // The first output is a snapshot; the new child may produce its banner later.
    let mut display = term();
    match first.next() {
        Message::Notification(Notification::TerminalOutput { session, data }) => {
            assert_eq!(session, "claude:alpha");
            Processor::<StdSyncHandler>::new().advance(&mut display, &data);
        }
        other => panic!("snapshot must follow open response: {other:?}"),
    }
    first.output_until(&mut display, "attached:alpha");
    first.ok(Request::TerminalInput {
        session: "claude:alpha".into(),
        data: b"persistent-marker\n".to_vec(),
    });
    first.output_until(&mut display, "persistent-marker");
    drop(first);
    let mut second = fixture.ready();
    let id = second.send(open("claude:alpha", 80, 24));
    assert_eq!(
        second.next(),
        Message::Response {
            id,
            result: Ok(json!({}))
        }
    );
    let mut restored = term();
    match second.next() {
        Message::Notification(Notification::TerminalOutput { data, .. }) => {
            Processor::<StdSyncHandler>::new().advance(&mut restored, &data)
        }
        other => panic!("expected snapshot: {other:?}"),
    }
    assert!(screen(&restored).contains("attached:alpha"));
    assert!(screen(&restored).contains("persistent-marker"));
    let mut third = fixture.ready();
    third.ok(open("claude:alpha", 80, 24));
    assert_eq!(
        fs::read_to_string(fixture.root.join("attached")).unwrap(),
        "alpha\n"
    );
    let mut outsider = fixture.ready();
    outsider.error(
        Request::TerminalInput {
            session: "claude:alpha".into(),
            data: b"unauthorized".to_vec(),
        },
        3,
    );
    outsider.error(
        Request::TerminalResize {
            session: "claude:alpha".into(),
            cols: 80,
            rows: 24,
        },
        3,
    );
    outsider.error(
        Request::TerminalClose {
            session: "claude:alpha".into(),
        },
        3,
    );
    second.error(
        Request::TerminalResize {
            session: "claude:alpha".into(),
            cols: 0,
            rows: 24,
        },
        5,
    );
    second.error(
        Request::TerminalResize {
            session: "claude:alpha".into(),
            cols: 80,
            rows: 0,
        },
        5,
    );
    second.ok(Request::TerminalResize {
        session: "claude:alpha".into(),
        cols: 100,
        rows: 30,
    });
    second.ok(Request::TerminalClose {
        session: "claude:alpha".into(),
    });
    second.error(
        Request::TerminalInput {
            session: "claude:alpha".into(),
            data: b"closed".to_vec(),
        },
        3,
    );
    // Only the remaining attached client sees the child exit.
    third.ok(Request::TerminalInput {
        session: "claude:alpha".into(),
        data: vec![4],
    });
    let deadline = Instant::now() + WAIT;
    loop {
        if let Message::Notification(Notification::TerminalExited { session, code }) = third.next()
        {
            assert_eq!(session, "claude:alpha");
            assert!(code.is_none() || code == Some(0));
            break;
        }
        assert!(Instant::now() < deadline);
    }
    third.ok(open("claude:alpha", 80, 24));
    let mut display = term();
    third.output_until(&mut display, "attached:alpha");
    assert_eq!(
        fs::read_to_string(fixture.root.join("attached")).unwrap(),
        "alpha\nalpha\n"
    );
}

#[test]
fn terminal_and_stop_errors_and_successful_stop_command() {
    let mut fixture = Fixture::new();
    fixture.sessions(false);
    fixture.start();
    let mut client = fixture.ready();
    client.error(open("claude:missing", 80, 24), 3);
    client.error(open("claude:archived", 80, 24), 4);
    client.error(open("claude:alpha", 0, 24), 5);
    client.error(open("claude:alpha", 80, 0), 5);
    client.error(
        Request::TerminalInput {
            session: "claude:alpha".into(),
            data: vec![],
        },
        3,
    );
    client.error(
        Request::TerminalResize {
            session: "claude:alpha".into(),
            cols: 80,
            rows: 24,
        },
        3,
    );
    client.error(
        Request::TerminalClose {
            session: "claude:alpha".into(),
        },
        3,
    );
    client.error(
        Request::SessionStop {
            session: "claude:missing".into(),
        },
        3,
    );
    client.error(
        Request::SessionStop {
            session: "claude:archived".into(),
        },
        4,
    );
    assert_eq!(
        client.ok(Request::SessionStop {
            session: "claude:alpha".into()
        }),
        json!({})
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("stopped")).unwrap(),
        "alpha\n"
    );
    let id = client.send(Request::SessionStop {
        session: "claude:fail-stop".into(),
    });
    let error = client.response(id).unwrap_err();
    assert_eq!(error.code, 7);
    assert!(error.message.contains("fixture stop failure"));
    // Discovery has already cached the session; removing the configured launcher
    // makes the subsequent attach fail without relying on a real CLI.
    fs::remove_file(fixture.root.join("fake-claude")).unwrap();
    client.error(open("claude:alpha", 80, 24), 7);
}

#[test]
fn malformed_requests_and_nonrequests_are_isolated_per_client() {
    let mut fixture = Fixture::new();
    fixture.start();
    let mut good = fixture.connect();
    good.hello();
    for (raw, code) in [
        (json!({"id":90,"method":"fixture/unknown","params":{}}), 6),
        (
            json!({"id":90,"method":"terminal/open","params":{"session":"x"}}),
            5,
        ),
    ] {
        writeln!(good.writer, "{raw}").unwrap();
        assert_eq!(good.response(90).unwrap_err().code, code);
    }
    for raw in [
        json!({"method":"fixture/unknown"}),
        json!({"method":"terminal/output","params":{}}),
        json!({"id":500,"result":{}}),
        json!({"method":"sessions/changed","params":{"sessions":[]}}),
    ] {
        writeln!(good.writer, "{raw}").unwrap();
    }
    let id = good.send(Request::SessionsList);
    assert!(matches!(good.next(), Message::Response { id: actual, result: Ok(_) } if actual == id));
    for bytes in [
        b"not-json\n".to_vec(),
        vec![0xff, b'\n'],
        [vec![b'x'; MAX_LINE_BYTES + 1], vec![b'\n']].concat(),
    ] {
        let mut bad = fixture.connect();
        // A server can reject an oversized line before the write completes.
        if let Err(error) = bad.writer.write_all(&bytes) {
            assert!(matches!(
                error.kind(),
                io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
            ));
        }
        bad.closed();
        good.ok(Request::SessionsList);
    }
}

#[test]
fn project_git_lookup_uses_official_arguments_and_is_cached_by_cwd() {
    let mut fixture = Fixture::new();
    fixture.sessions(false);
    let git = fixture.root.join("fake-git");
    fs::write(&git, r##"#!/bin/sh
base=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
[ "$1" = -C ] && [ "$2" = "$base" ] && [ "$3" = rev-parse ] && [ "$4" = --path-format=absolute ] && [ "$5" = --git-common-dir ] || exit 48
printf 'lookup\n' >> "$base/git-calls"
printf '%s/shared/.git\n' "$base"
"##).unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = fixture.config();
    config.git_program = Some(git.to_str().unwrap().into());
    fixture.daemon = Some(Daemon::start(config).unwrap());
    let mut client = fixture.ready();
    let sessions: SessionsResult =
        serde_json::from_value(client.ok(Request::SessionsList)).unwrap();
    for session in sessions.sessions {
        assert_eq!(
            session.project.root,
            fixture.root.join("shared").to_str().unwrap()
        );
        assert_eq!(session.project.name, "shared");
    }
    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        fs::read_to_string(fixture.root.join("git-calls")).unwrap(),
        "lookup\n"
    );
}

#[test]
fn failed_git_and_disabled_git_fall_back_to_git_marker_file_or_directory() {
    for directory_marker in [false, true] {
        let mut fixture = Fixture::new();
        let nested = fixture.root.join("nested");
        fs::create_dir(&nested).unwrap();
        if directory_marker {
            fs::create_dir(fixture.root.join(".git")).unwrap();
        } else {
            fs::write(fixture.root.join(".git"), "fixture marker").unwrap();
        }
        let agents: Vec<_> = ["alpha", "beta", "gamma"]
            .into_iter()
            .map(|id| json!({"id":id,"cwd":nested.to_str().unwrap()}))
            .collect();
        fs::write(
            fixture.root.join("agents.json"),
            serde_json::to_vec(&agents).unwrap(),
        )
        .unwrap();
        let mut config = fixture.config();
        if directory_marker {
            config.git_program = Some(fixture.root.join("missing-git").to_str().unwrap().into());
        }
        fixture.daemon = Some(Daemon::start(config).unwrap());
        let mut client = fixture.ready();
        let result: SessionsResult =
            serde_json::from_value(client.ok(Request::SessionsList)).unwrap();
        for session in result.sessions {
            assert_eq!(session.project.root, fixture.root.to_str().unwrap());
        }
    }
}
