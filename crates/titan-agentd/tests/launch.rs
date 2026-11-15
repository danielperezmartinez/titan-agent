//! Launch acceptance tests use isolated fake official CLIs and no network.
#![cfg(unix)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use titan_agent_core::launch::LaunchSpec;
use titan_agent_core::session::AgentCli;

const CLAUDE: &str = r#"#!/bin/sh
case "$1" in
  agents) cat "$FAKE_DIR/agents.json"; exit 0 ;;
  --bg)
    printf 'call\n' >> "$FAKE_DIR/claude.calls"
    printf '%s\0' "$PWD" "$@" >> "$FAKE_DIR/claude.args"
    if [ -f "$FAKE_DIR/fail" ]; then
      echo 'example launch refused' >&2
      exit 17
    fi
    printf 'backgrounded · ab12cd34 · example session\n'
    ;;
  *) exit 19 ;;
esac
"#;

const CODEX: &str = r#"#!/bin/sh
if [ "$1" = app-server ] && [ "$2" = daemon ] && [ "$3" = version ]; then
  echo '{"status":"stopped"}'
  exit 0
fi
printf 'call\n' >> "$FAKE_DIR/codex.calls"
printf '%s\0' "$PWD" "$@" >> "$FAKE_DIR/codex.args"
if ! [ -t 0 ] || ! [ -t 1 ]; then
  echo 'missing interactive terminal'
  exit 23
fi
stty -echo 2>/dev/null
printf 'example TUI ready\r\n'
while IFS= read -r line; do printf 'received:%s\r\n' "$line"; done
"#;

struct Fixture {
    dir: PathBuf,
    socket: PathBuf,
    daemon: Child,
}
impl Fixture {
    fn start(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ta-launch-{name}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("work space")).unwrap();
        std::fs::write(dir.join("agents.json"), "[]").unwrap();
        for (name, body) in [("claude", CLAUDE), ("codex", CODEX)] {
            let path = dir.join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let socket = dir.join("run/agentd.sock");
        let daemon = Command::new(env!("CARGO_BIN_EXE_titan-agentd"))
            .env("TITAN_AGENTD_SOCKET", &socket)
            .env("TITAN_AGENTD_CLAUDE", dir.join("claude"))
            .env("TITAN_AGENTD_CODEX", dir.join("codex"))
            .env("TITAN_AGENTD_POLL_MS", "100")
            .env("HOME", &dir)
            .env("FAKE_DIR", &dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let fixture = Self {
            dir,
            socket,
            daemon,
        };
        let start = Instant::now();
        while !fixture.socket.exists() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "daemon startup timed out"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        fixture
    }
    fn connect(&self) -> Connection {
        let writer = UnixStream::connect(&self.socket).unwrap();
        writer
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut c = Connection {
            reader: BufReader::new(writer.try_clone().unwrap()),
            writer,
            seen: vec![],
        };
        c.send(
            json!({"id":1,"method":"hello","params":{"protocol":1,"client":"launch-acceptance"}}),
        );
        assert_eq!(c.response(1)["result"]["protocol"], 1);
        c.send(json!({"id":2,"method":"subscribe"}));
        c.until(|m| m["event"] == "sessions");
        c
    }
    fn spec(&self, agent: AgentCli, brief: &str) -> LaunchSpec {
        LaunchSpec {
            agent,
            cwd: self.dir.join("work space").to_str().unwrap().into(),
            model: if agent == AgentCli::ClaudeCode {
                "haiku"
            } else {
                "gpt-6.1-sol"
            }
            .into(),
            effort: "low".into(),
            permission: if agent == AgentCli::ClaudeCode {
                "plan"
            } else {
                "workspace-write"
            }
            .into(),
            brief: brief.into(),
        }
    }
    fn calls(&self, agent: &str) -> usize {
        match std::fs::read_to_string(self.dir.join(format!("{agent}.calls"))) {
            Ok(s) => s.lines().count(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(e) => panic!("cannot read invocation log: {e}"),
        }
    }
    fn assert_execution(&self, agent: &str, s: &LaunchSpec) {
        assert_eq!(self.calls(agent), 1);
        let mut expected = Vec::new();
        for arg in std::iter::once(s.cwd.clone()).chain(s.argv().into_iter().skip(1)) {
            expected.extend_from_slice(arg.as_bytes());
            expected.push(0);
        }
        assert_eq!(
            std::fs::read(self.dir.join(format!("{agent}.args"))).unwrap(),
            expected
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Connection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    seen: Vec<Value>,
}
impl Connection {
    fn send(&mut self, value: Value) {
        writeln!(self.writer, "{value}").unwrap();
    }
    fn until(&mut self, matches: impl Fn(&Value) -> bool) -> Value {
        if let Some(m) = self.seen.iter().find(|m| matches(m)) {
            return m.clone();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(
                Instant::now() < deadline,
                "expected protocol message did not arrive"
            );
            let mut line = String::new();
            assert!(
                self.reader.read_line(&mut line).unwrap() > 0,
                "daemon closed connection"
            );
            let m: Value = serde_json::from_str(&line).unwrap();
            self.seen.push(m.clone());
            if matches(&m) {
                return m;
            }
        }
    }
    fn response(&mut self, id: u64) -> Value {
        self.until(|m| m["id"] == id)
    }
    fn launch(&mut self, id: u64, token: &str, s: &LaunchSpec) -> Value {
        self.send(request(id, token, s));
        self.response(id)
    }
    fn output_until(&mut self, terminal: &str, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut output = Vec::new();
            for m in &self.seen {
                if m["event"] == "output" && m["params"]["terminal"] == terminal {
                    output.extend(
                        titan_agent_core::protocol::decode_bytes(
                            m["params"]["data"].as_str().unwrap(),
                        )
                        .unwrap(),
                    );
                }
            }
            if String::from_utf8_lossy(&output).contains(needle) {
                return;
            }
            assert!(Instant::now() < deadline, "terminal output timed out");
            // Consume a fresh message; output may be split across arbitrary events.
            let mut line = String::new();
            assert!(self.reader.read_line(&mut line).unwrap() > 0);
            self.seen.push(serde_json::from_str(&line).unwrap());
        }
    }
}
fn contains_string(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(s) => s == expected,
        Value::Array(items) => items.iter().any(|v| contains_string(v, expected)),
        Value::Object(fields) => fields.values().any(|v| contains_string(v, expected)),
        _ => false,
    }
}
fn request(id: u64, token: &str, s: &LaunchSpec) -> Value {
    json!({"id":id,"method":"launch","params":{"token":token,"spec":s,"command":s.command_line()}})
}
fn error(answer: Value, expected: &str) {
    assert_eq!(answer["error"]["code"], expected, "{answer}");
    assert!(
        answer["error"]["message"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
}

#[test]
fn claude_launch_runs_the_displayed_argv_and_resolves_the_provisional_session() {
    let f = Fixture::start("claude");
    let mut c = f.connect();
    let s = f.spec(
        AgentCli::ClaudeCode,
        "  Repair 'example' ; $(touch injected)\nKeep literal spacing.  ",
    );
    let answer = c.launch(3, "click-claude", &s);
    assert_eq!(
        answer["result"],
        json!({"launch":"launch:1","agent":"claude_code","terminal":null,"cli_id":"ab12cd34"})
    );
    f.assert_execution("claude", &s);
    assert!(!PathBuf::from(&s.cwd).join("injected").exists());
    let snapshot = c.until(|m| {
        m["event"] == "sessions"
            && m["params"]["sessions"]
                .as_array()
                .is_some_and(|a| a.iter().any(|s| s["key"] == "launch:1"))
    });
    let provisional = snapshot["params"]["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == "launch:1")
        .unwrap();
    assert_eq!(provisional["agent"], "claude_code");
    assert_eq!(provisional["cwd"], s.cwd);
    assert_eq!(provisional["model"], s.model);
    assert_eq!(provisional["name"], s.session_name());
    assert!(
        snapshot["params"]["launches"]
            .as_array()
            .unwrap()
            .contains(&json!({"launch":"launch:1","session":null}))
    );
    let real = "cl:ab12cd34-0000-4000-8000-000000000001";
    let agents = json!([{"id":"ab12cd34","pid":1,"cwd":s.cwd,"kind":"background","startedAt":1700000000000_i64,
        "sessionId":"ab12cd34-0000-4000-8000-000000000001","name":s.session_name(),"status":"busy"}]);
    // Rename avoids exposing a partially written discovery response to a poll.
    std::fs::write(f.dir.join("agents.next"), agents.to_string()).unwrap();
    std::fs::rename(f.dir.join("agents.next"), f.dir.join("agents.json")).unwrap();
    c.until(|m| {
        m["event"] == "sessions"
            && m["params"]["launches"]
                .as_array()
                .is_some_and(|a| a.contains(&json!({"launch":"launch:1","session":real})))
    });
}

#[test]
fn codex_launch_uses_an_interactive_pty_with_output_input_and_snapshot_association() {
    let f = Fixture::start("codex");
    let mut c = f.connect();
    let s = f.spec(
        AgentCli::Codex,
        "  Inspect 'example'\nDo not change the brief.  ",
    );
    let answer = c.launch(3, "click-codex", &s);
    assert_eq!(answer["result"]["launch"], "launch:1");
    assert_eq!(answer["result"]["agent"], "codex");
    assert_eq!(answer["result"]["cli_id"], Value::Null);
    let terminal = answer["result"]["terminal"].as_str().unwrap();
    c.output_until(terminal, "example TUI ready");
    f.assert_execution("codex", &s);
    c.send(json!({"method":"input","params":{"terminal":terminal,"data":titan_agent_core::protocol::encode_bytes(b"example input\r")}}));
    c.output_until(terminal, "received:example input");
    // The specification does not name the terminal association fields. Require
    // both IDs in one terminal record without choosing those field names.
    c.until(|m| {
        m["event"] == "sessions"
            && m["params"]["terminals"].as_array().is_some_and(|a| {
                a.iter()
                    .any(|t| contains_string(t, terminal) && contains_string(t, "launch:1"))
            })
    });
    c.send(json!({"id":4,"method":"close_terminal","params":{"terminal":terminal}}));
    c.until(|m| m["event"] == "terminal_closed" && m["params"]["terminal"] == terminal);
}

#[test]
fn invalid_requests_are_rejected_before_any_cli_execution() {
    let f = Fixture::start("reject");
    let mut c = f.connect();
    let base = f.spec(AgentCli::ClaudeCode, "Example task");
    // Malformed JSON has no recoverable request ID; only the error code is fixed.
    c.writer.write_all(b"{\n").unwrap();
    error(
        c.until(|m| m["error"]["code"] == "bad_request"),
        "bad_request",
    );
    let mut cases = Vec::new();
    for field in ["repeat", "delay", "schedule"] {
        let mut r = request(0, field, &base);
        r["params"][field] = json!(1);
        cases.push((r, "bad_request"));
    }
    let mut r = request(0, "unknown-spec-field", &base);
    r["params"]["spec"]["template"] = json!("example");
    cases.push((r, "bad_request"));
    for (field, value, code) in [
        ("brief", " \n\t", "empty_brief"),
        ("brief", "hello\0", "bad_brief"),
        ("cwd", "relative", "bad_folder"),
        ("model", "-unsafe", "bad_option"),
        ("permission", "bypassPermissions", "bad_option"),
    ] {
        let mut r = request(0, field, &base);
        r["params"]["spec"][field] = json!(value);
        // A stale command also proves validation precedes command comparison.
        cases.push((r, code));
    }
    let mut r = request(0, "long", &base);
    r["params"]["spec"]["brief"] = json!("x".repeat(32_001));
    cases.push((r, "brief_too_long"));
    for cwd in [f.dir.join("missing-folder"), f.dir.join("agents.json")] {
        let mut s = base.clone();
        s.cwd = cwd.to_str().unwrap().into();
        cases.push((request(0, "folder", &s), "bad_folder"));
    }
    let mut r = request(0, "mismatch", &base);
    r["params"]["command"] = json!(format!("{} ", base.command_line()));
    cases.push((r, "command_mismatch"));
    for (index, (mut r, code)) in cases.into_iter().enumerate() {
        let id = index as u64 + 10;
        r["id"] = json!(id);
        // Independent clicks avoid making error order depend on earlier tokens.
        r["params"]["token"] = json!(format!("rejected-click-{id}"));
        c.send(r);
        error(c.response(id), code);
    }
    assert_eq!(f.calls("claude"), 0);
    assert_eq!(f.calls("codex"), 0);
}

#[test]
fn duplicate_tokens_and_recent_identical_briefs_do_not_launch_again() {
    let f = Fixture::start("duplicate");
    let mut c = f.connect();
    let first = f.spec(AgentCli::ClaudeCode, "First example task");
    assert_eq!(
        c.launch(3, "first-click", &first)["result"]["launch"],
        "launch:1"
    );
    let second = f.spec(AgentCli::ClaudeCode, "Different example task");
    // Reconnect: tokens belong to the daemon, not a single client connection.
    let mut other = f.connect();
    error(other.launch(4, "first-click", &second), "duplicate_launch");
    error(
        other.launch(5, "different-click", &first),
        "duplicate_launch",
    );
    assert_eq!(f.calls("claude"), 1);
    assert_eq!(
        other.launch(6, "second-click", &second)["result"]["launch"],
        "launch:2"
    );
    assert_eq!(f.calls("claude"), 2);
}

#[test]
fn failed_cli_is_run_once_reports_its_message_and_consumes_its_token() {
    let f = Fixture::start("failure");
    let mut c = f.connect();
    std::fs::write(f.dir.join("fail"), "fail this launch").unwrap();
    let s = f.spec(AgentCli::ClaudeCode, "Example failing task");
    let answer = c.launch(3, "failed-click", &s);
    error(answer.clone(), "launch_failed");
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("example launch refused")
    );
    // A bounded observation covers several polls; no finite test can rule out
    // arbitrarily delayed retries. The protocol response must already be final.
    std::thread::sleep(Duration::from_millis(400));
    f.assert_execution("claude", &s);
    std::fs::remove_file(f.dir.join("fail")).unwrap();
    let different = f.spec(AgentCli::ClaudeCode, "A different example task");
    error(c.launch(4, "failed-click", &different), "duplicate_launch");
    assert_eq!(f.calls("claude"), 1);
}
