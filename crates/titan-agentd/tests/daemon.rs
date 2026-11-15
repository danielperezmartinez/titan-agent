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
  --bg)
    pwd > "$FAKE_DIR/launch-cwd"
    { for arg in "$@"; do printf '%s\0' "$arg"; done; printf '\001\0'; } >> "$FAKE_DIR/launched"
    cp "$FAKE_DIR/agents-after.json" "$FAKE_DIR/agents.json" 2>/dev/null
    printf 'backgrounded \302\267 beef0001 \302\267 %s\n' "$3" ;;
esac
"#;

const FAKE_CODEX: &str = r#"#!/bin/sh
case "$1" in
  --remote)
    pwd > "$FAKE_DIR/codex-cwd"
    { for arg in "$@"; do printf '%s\0' "$arg"; done; printf '\001\0'; } >> "$FAKE_DIR/codex-launched"
    stty -echo 2>/dev/null
    printf 'codex tui ready\r\n'
    while IFS= read -r line; do printf 'tui got:%s\r\n' "$line"; done ;;
  *) echo '{"status":"stopped"}' ;;
esac
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
        std::fs::create_dir_all(dir.join("proyecto")).unwrap();
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

// ----- launching (ADR-0002) --------------------------------------------------

use titan_agent_core::launch::LaunchSpec;
use titan_agent_core::session::AgentCli;

fn spec(fixture: &Fixture, agent: AgentCli, brief: &str) -> LaunchSpec {
    let (model, access) = match agent {
        AgentCli::ClaudeCode => ("haiku", "plan"),
        AgentCli::Codex => ("gpt-6-luna", "read-only"),
    };
    LaunchSpec {
        agent,
        model: model.into(),
        effort: "low".into(),
        access: access.into(),
        cwd: fixture.dir.join("proyecto").to_string_lossy().into_owned(),
        brief: brief.into(),
    }
}

fn launch(c: &mut Connection, id: u64, spec: &LaunchSpec, shown: &str, click: &str) -> Value {
    c.send(json!({"id": id, "method": "launch", "params": {
        "spec": spec, "shown": shown, "click": click, "cols": 100, "rows": 30
    }}));
    c.until(|m| m["id"] == id)
}

/// The command line the daemon expects (its HOME is the fixture folder).
fn shown(fixture: &Fixture, spec: &LaunchSpec) -> String {
    spec.command_line(Some(&fixture.dir.to_string_lossy()))
}

/// The runs recorded by a fake CLI: NUL-separated arguments, each run
/// closed by a `\x01` record.
fn args(path: &Path) -> Vec<Vec<String>> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let mut runs = vec![Vec::new()];
    for part in bytes.split(|b| *b == 0).filter(|part| !part.is_empty()) {
        if part == b"\x01" {
            runs.push(Vec::new());
        } else if let Some(run) = runs.last_mut() {
            run.push(String::from_utf8_lossy(part).into_owned());
        }
    }
    runs.pop();
    runs
}

#[test]
fn launching_claude_runs_the_official_cli_once_with_the_users_brief() {
    let fixture = Fixture::start("launch-claude");
    let after = AGENTS.replace(
        "\n]",
        r#",
  {"id":"beef0001","pid":3,"cwd":"/tmp","kind":"background","startedAt":1700000002000,
   "sessionId":"beef0001-0000-4000-8000-000000000003","name":"Revisa","status":"busy"}
]"#,
    );
    std::fs::write(fixture.dir.join("agents-after.json"), after).unwrap();
    let mut c = fixture.connect();
    c.hello();
    c.send(json!({"id": 2, "method": "subscribe"}));

    let brief = "Revisa el README.\n\n- sin tocar \"nada\" más, $HOME";
    let spec = spec(&fixture, AgentCli::ClaudeCode, brief);
    let answer = launch(&mut c, 3, &spec, &shown(&fixture, &spec), "click-1");
    assert_eq!(answer["result"]["cli_id"], "beef0001", "{answer}");
    assert_eq!(answer["result"]["command"], shown(&fixture, &spec));

    // The official CLI got exactly the argv of the spec, in the folder.
    let runs = args(&fixture.dir.join("launched"));
    assert_eq!(runs, vec![spec.argv()[1..].to_vec()]);
    assert_eq!(runs[0].last().unwrap(), brief);
    let cwd = std::fs::read_to_string(fixture.dir.join("launch-cwd")).unwrap();
    assert_eq!(cwd.trim(), spec.cwd);

    // The new session shows up at once (the launch asks for a poll).
    c.until(|m| {
        m["event"] == "sessions"
            && m["params"]["sessions"].as_array().is_some_and(|s| {
                s.iter()
                    .any(|s| s["cli_id"] == "beef0001" && s["launched_here"] == true)
            })
    });

    // Same click again, or the same brief right after: refused, not re-run.
    let again = launch(&mut c, 4, &spec, &shown(&fixture, &spec), "click-1");
    assert_eq!(again["error"]["code"], "duplicate_launch");
    let twice = launch(&mut c, 5, &spec, &shown(&fixture, &spec), "click-2");
    assert_eq!(twice["error"]["code"], "duplicate_launch");
    assert_eq!(args(&fixture.dir.join("launched")).len(), 1);
}

#[test]
fn launches_without_the_users_brief_or_the_shown_command_are_refused() {
    let fixture = Fixture::start("launch-refused");
    let mut c = fixture.connect();
    c.hello();

    let blank = spec(&fixture, AgentCli::ClaudeCode, "   \n ");
    let answer = launch(&mut c, 2, &blank, &shown(&fixture, &blank), "c1");
    assert_eq!(answer["error"]["code"], "empty_brief");

    let good = spec(&fixture, AgentCli::ClaudeCode, "Haz una cosa");
    let mut other = good.clone();
    other.brief = "Haz otra cosa".into();
    let answer = launch(&mut c, 3, &good, &shown(&fixture, &other), "c2");
    assert_eq!(answer["error"]["code"], "command_mismatch");

    let mut bad = good.clone();
    bad.access = "bypassPermissions".into();
    let answer = launch(&mut c, 4, &bad, &shown(&fixture, &bad), "c3");
    assert_eq!(answer["error"]["code"], "invalid_option");

    let answer = launch(&mut c, 5, &good, &shown(&fixture, &good), " ");
    assert_eq!(answer["error"]["code"], "missing_click");

    let mut gone = good.clone();
    gone.cwd = fixture.dir.join("no-existe").to_string_lossy().into_owned();
    let answer = launch(&mut c, 6, &gone, &shown(&fixture, &gone), "c4");
    assert_eq!(answer["error"]["code"], "no_folder");

    assert!(
        args(&fixture.dir.join("launched")).is_empty(),
        "nothing was launched"
    );

    // A refused launch does not block the next click with the same brief.
    let answer = launch(&mut c, 7, &good, &shown(&fixture, &good), "c5");
    assert_eq!(answer["result"]["cli_id"], "beef0001", "{answer}");
}

#[test]
fn launching_codex_opens_its_tui_in_a_daemon_terminal() {
    let fixture = Fixture::start("launch-codex");
    let mut c = fixture.connect();
    c.hello();
    c.send(json!({"id": 2, "method": "subscribe"}));

    let spec = spec(&fixture, AgentCli::Codex, "-x Añade un test");
    let answer = launch(&mut c, 3, &spec, &shown(&fixture, &spec), "click-1");
    let terminal = answer["result"]["terminal"]
        .as_str()
        .unwrap_or_else(|| panic!("{answer}"))
        .to_string();
    c.output_until(&terminal, "codex tui ready");

    // The terminal is listed under its provisional key and takes input.
    c.until(|m| {
        m["event"] == "sessions"
            && m["params"]["terminals"].as_array().is_some_and(|t| {
                t.iter().any(|t| {
                    t["terminal"] == terminal.as_str()
                        && t["session"] == format!("launch:{terminal}").as_str()
                })
            })
    });
    let data = titan_agent_core::protocol::encode_bytes(b"sigue\r");
    c.send(json!({"method": "input", "params": {"terminal": terminal, "data": data}}));
    c.output_until(&terminal, "tui got:sigue");

    let runs = args(&fixture.dir.join("codex-launched"));
    assert_eq!(runs, vec![spec.argv()[1..].to_vec()]);
    assert_eq!(&runs[0][runs[0].len() - 2..], ["--", "-x Añade un test"]);
    let cwd = std::fs::read_to_string(fixture.dir.join("codex-cwd")).unwrap();
    assert_eq!(cwd.trim(), spec.cwd);

    let again = launch(&mut c, 4, &spec, &shown(&fixture, &spec), "click-1");
    assert_eq!(again["error"]["code"], "duplicate_launch");
    assert_eq!(args(&fixture.dir.join("codex-launched")).len(), 1);
}
