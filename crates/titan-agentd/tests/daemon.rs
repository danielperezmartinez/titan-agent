//! End-to-end tests of titan-agentd with fake `claude` and `codex`
//! executables (shell scripts), so no real agent session is touched.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const AGENTS: &str = r#"[
  {"id":"ab12cd34","pid":1,"cwd":"/tmp","kind":"background","startedAt":1700000000000,
   "sessionId":"ab12cd34-0000-4000-8000-000000000001","name":"prueba: kite","status":"busy"},
  {"pid":2,"cwd":"/tmp","kind":"interactive","startedAt":1700000001000,
   "sessionId":"00000000-0000-4000-8000-000000000002","name":"notes-1a","status":"idle"}
]"#;

const FAKE_CLAUDE: &str = r#"#!/bin/sh
case "$1" in
  agents) cat "$FAKE_DIR/agents.json" ;;
  attach)
    stty -echo 2>/dev/null
    printf 'attached to %s\r\n' "$2"
    while IFS= read -r line; do printf 'got:%s\r\n' "$line"; done ;;
  stop) echo "$2" >> "$FAKE_DIR/stopped" ;;
esac
"#;

const FAKE_CODEX: &str = r#"#!/bin/sh
echo '{"status":"stopped"}'
"#;

struct Fixture {
    dir: PathBuf,
    socket: PathBuf,
    daemon: Child,
}

impl Fixture {
    fn start(name: &str) -> Fixture {
        let dir =
            std::env::temp_dir().join(format!("titan-agentd-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("agents.json"), AGENTS).unwrap();
        for (file, body) in [("claude", FAKE_CLAUDE), ("codex", FAKE_CODEX)] {
            let path = dir.join(file);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let socket = dir.join("run").join("agentd.sock");
        let daemon = spawn_daemon(&dir, &socket);
        wait_for(|| socket.exists());
        Fixture {
            dir,
            socket,
            daemon,
        }
    }

    fn connect(&self) -> Connection {
        let stream = UnixStream::connect(&self.socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        Connection {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn spawn_daemon(dir: &Path, socket: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_titan-agentd"))
        .env("TITAN_AGENTD_SOCKET", socket)
        .env("TITAN_AGENTD_CLAUDE", dir.join("claude"))
        .env("TITAN_AGENTD_CODEX", dir.join("codex"))
        .env("TITAN_AGENTD_POLL_MS", "200")
        .env("HOME", dir)
        .env("FAKE_DIR", dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn wait_for(mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(start.elapsed() < Duration::from_secs(10), "timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}

struct Connection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Connection {
    fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).unwrap();
    }

    fn next(&mut self) -> Value {
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "the daemon closed the connection");
        serde_json::from_str(&line).unwrap()
    }

    /// Reads until a message matches, returning it.
    fn until(&mut self, mut matches: impl FnMut(&Value) -> bool) -> Value {
        loop {
            let message = self.next();
            if matches(&message) {
                return message;
            }
        }
    }

    fn hello(&mut self) {
        self.send(json!({"id": 1, "method": "hello", "params": {"protocol": 1, "client": "test"}}));
        let answer = self.until(|m| m["id"] == 1);
        assert_eq!(answer["result"]["protocol"], 1);
    }

    /// Collects terminal output until `text` shows up.
    fn output_until(&mut self, terminal: &str, text: &str) -> String {
        let mut seen = String::new();
        while !seen.contains(text) {
            let message = self.next();
            if message["event"] == "output" && message["params"]["terminal"] == terminal {
                let data = message["params"]["data"].as_str().unwrap();
                let bytes = titan_agent_core::protocol::decode_bytes(data).unwrap();
                seen.push_str(&String::from_utf8_lossy(&bytes));
            }
        }
        seen
    }
}

const BACKGROUND: &str = "cl:ab12cd34-0000-4000-8000-000000000001";
const INTERACTIVE: &str = "cl:00000000-0000-4000-8000-000000000002";

#[test]
fn hello_is_required_and_versioned() {
    let fixture = Fixture::start("hello");
    let mut c = fixture.connect();
    c.send(json!({"id": 5, "method": "subscribe"}));
    assert_eq!(c.until(|m| m["id"] == 5)["error"]["code"], "hello_required");
    c.send(json!({"id": 6, "method": "hello", "params": {"protocol": 99, "client": "t"}}));
    assert_eq!(
        c.until(|m| m["id"] == 6)["error"]["code"],
        "protocol_mismatch"
    );
    c.send(json!({"id": 7, "method": "bogus"}));
    assert_eq!(c.until(|m| m["id"] == 7)["error"]["code"], "bad_request");
    c.hello();
}

#[test]
fn sessions_are_pushed_with_source_health() {
    let fixture = Fixture::start("sessions");
    let mut c = fixture.connect();
    c.hello();
    c.send(json!({"id": 2, "method": "subscribe"}));
    let snapshot = c.until(|m| {
        m["event"] == "sessions"
            && m["params"]["sessions"]
                .as_array()
                .is_some_and(|s| s.len() == 2)
    });
    let params = &snapshot["params"];
    let background = params["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == BACKGROUND)
        .unwrap();
    assert_eq!(background["state"], "working");
    assert_eq!(
        background["attach"],
        json!(["claude", "attach", "ab12cd34"])
    );
    assert!(background["state_since_ms"].is_i64());
    let sources = params["sources"].as_array().unwrap();
    assert_eq!(sources[0]["agent"], "claude_code");
    assert_eq!(sources[0]["ok"], true);
    assert_eq!(sources[1]["agent"], "codex");
    assert_eq!(sources[1]["ok"], false);

    // A change in the CLI's answer reaches the clients within a poll.
    std::fs::write(
        fixture.dir.join("agents.json"),
        AGENTS.replace(r#""status":"busy""#, r#""state":"blocked""#),
    )
    .unwrap();
    c.until(|m| {
        m["event"] == "sessions"
            && m["params"]["sessions"].as_array().is_some_and(|s| {
                s.iter()
                    .any(|s| s["key"] == BACKGROUND && s["state"] == "needs_you")
            })
    });
}

#[test]
fn terminals_survive_their_clients_and_replay_history() {
    let fixture = Fixture::start("terminal");
    let mut first = fixture.connect();
    first.hello();
    first.send(json!({"id": 2, "method": "subscribe"}));
    first.until(|m| {
        m["event"] == "sessions"
            && m["params"]["sessions"]
                .as_array()
                .is_some_and(|s| !s.is_empty())
    });

    first.send(json!({"id": 3, "method": "attach", "params": {"session": BACKGROUND, "cols": 80, "rows": 24}}));
    let attached = first.until(|m| m["id"] == 3);
    assert_eq!(attached["result"]["created"], true);
    let terminal = attached["result"]["terminal"].as_str().unwrap().to_string();
    first.output_until(&terminal, "attached to ab12cd34");

    let data = titan_agent_core::protocol::encode_bytes(b"hola\r");
    first.send(json!({"method": "input", "params": {"terminal": terminal, "data": data}}));
    first.output_until(&terminal, "got:hola");
    first.send(
        json!({"method": "resize", "params": {"terminal": terminal, "cols": 100, "rows": 30}}),
    );

    // The window closes: the terminal stays in the daemon.
    drop(first);
    let mut second = fixture.connect();
    second.hello();
    second.send(json!({"id": 4, "method": "attach", "params": {"session": BACKGROUND, "cols": 100, "rows": 30}}));
    let again = second.until(|m| m["id"] == 4);
    assert_eq!(again["result"]["created"], false);
    assert_eq!(again["result"]["terminal"], terminal.as_str());
    let replay = second.output_until(&terminal, "got:hola");
    assert!(
        replay.contains("attached to ab12cd34"),
        "history is replayed: {replay:?}"
    );

    // Ending the terminal tells the client.
    second.send(json!({"id": 5, "method": "close_terminal", "params": {"terminal": terminal}}));
    second
        .until(|m| m["event"] == "terminal_closed" && m["params"]["terminal"] == terminal.as_str());
}

#[test]
fn interactive_sessions_cannot_be_attached_and_stop_uses_the_cli() {
    let fixture = Fixture::start("stop");
    let mut c = fixture.connect();
    c.hello();
    c.send(json!({"id": 2, "method": "subscribe"}));
    c.until(|m| {
        m["event"] == "sessions"
            && m["params"]["sessions"]
                .as_array()
                .is_some_and(|s| s.len() == 2)
    });

    c.send(json!({"id": 3, "method": "attach", "params": {"session": INTERACTIVE, "cols": 80, "rows": 24}}));
    assert_eq!(c.until(|m| m["id"] == 3)["error"]["code"], "not_attachable");
    c.send(json!({"id": 4, "method": "attach", "params": {"session": "cl:nope", "cols": 80, "rows": 24}}));
    assert_eq!(
        c.until(|m| m["id"] == 4)["error"]["code"],
        "unknown_session"
    );

    c.send(json!({"id": 5, "method": "stop", "params": {"session": INTERACTIVE}}));
    assert_eq!(c.until(|m| m["id"] == 5)["error"]["code"], "stop_failed");
    c.send(json!({"id": 6, "method": "stop", "params": {"session": BACKGROUND}}));
    assert!(c.until(|m| m["id"] == 6)["result"].is_object());
    let stopped = std::fs::read_to_string(fixture.dir.join("stopped")).unwrap();
    assert_eq!(stopped.trim(), "ab12cd34");
}

#[test]
fn only_one_daemon_per_socket() {
    let fixture = Fixture::start("single");
    let mut second = spawn_daemon(&fixture.dir, &fixture.socket);
    let status = second.wait().unwrap();
    assert!(
        !status.success(),
        "a second daemon on the same socket must exit"
    );
    // The first one still answers.
    let mut c = fixture.connect();
    c.hello();
}
