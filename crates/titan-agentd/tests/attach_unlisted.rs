//! Codex threads the daemon does not list, against a fake app-server:
//! «tomar el control» resolves one with `thread/read` and opens it with the
//! same `codex resume` as a listed thread, and a transcript whose read is cut
//! by the app-server is reported as `codex_unavailable`.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use titan_agent_core::protocol::decode_bytes;

const FAKE_CODEX: &str = r#"#!/bin/sh
if [ "$1" = app-server ]; then
  printf '{"status":"running","socketPath":"%s/codex.sock"}\n' "$FAKE_DIR"
  exit 0
fi
printf 'codex %s\r\n' "$*"
while IFS= read -r line; do :; done
"#;

/// Answers JSON-RPC like the Codex app-server: it lists nothing, knows one
/// thread, `known`, and drops the connection when asked for `cut`.
fn serve(stream: UnixStream, project: PathBuf) {
    let Ok(mut ws) = tungstenite::accept(stream) else {
        return;
    };
    while let Ok(message) = ws.read() {
        let tungstenite::Message::Text(text) = message else {
            continue;
        };
        let request: Value = serde_json::from_str(text.as_ref()).unwrap();
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let thread = request["params"]["threadId"].as_str().unwrap_or("");
        if thread == "cut" {
            return;
        }
        let answer = match request["method"].as_str().unwrap_or("") {
            "thread/read" if thread == "known" => json!({"id": id, "result": {"thread": {
                "id": "known", "cwd": project, "source": "exec",
                "status": {"type": "notLoaded"}, "preview": "revisión"}}}),
            "thread/read" => json!({"id": id, "error": {"code": -32600,
                "message": format!("thread not loaded: {thread}")}}),
            "account/rateLimits/read" => json!({"id": id, "result": {}}),
            _ => json!({"id": id, "result": {"data": []}}),
        };
        if ws
            .send(tungstenite::Message::text(answer.to_string()))
            .is_err()
        {
            return;
        }
    }
}

struct Fixture {
    dir: PathBuf,
    daemon: Child,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn start(name: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!(
        "titan-agentd-unlisted-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("project")).unwrap();
    let write_exe = |name: &str, body: &str| {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    write_exe("codex", FAKE_CODEX);
    write_exe("claude", "#!/bin/sh\necho '[]'\n");
    let listener = UnixListener::bind(dir.join("codex.sock")).unwrap();
    let project = dir.join("project");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let project = project.clone();
            std::thread::spawn(move || serve(stream, project));
        }
    });
    let daemon = Command::new(env!("CARGO_BIN_EXE_titan-agentd"))
        .env("TITAN_AGENTD_SOCKET", dir.join("agentd.sock"))
        .env("TITAN_AGENTD_CLAUDE", dir.join("claude"))
        .env("TITAN_AGENTD_CODEX", dir.join("codex"))
        .env("TITAN_AGENTD_POLL_MS", "200")
        .env("FAKE_DIR", &dir)
        .env("HOME", &dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Fixture { dir, daemon }
}

fn connect(socket: &Path) -> (BufReader<UnixStream>, UnixStream) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let stream = loop {
        if let Ok(stream) = UnixStream::connect(socket) {
            break stream;
        }
        assert!(Instant::now() < deadline, "the daemon did not start");
        std::thread::sleep(Duration::from_millis(20));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    (BufReader::new(stream.try_clone().unwrap()), stream)
}

/// Reads lines until `pick` returns something.
fn wait_for<T>(reader: &mut BufReader<UnixStream>, mut pick: impl FnMut(&Value) -> Option<T>) -> T {
    loop {
        let mut line = String::new();
        assert_ne!(reader.read_line(&mut line).unwrap(), 0, "daemon closed");
        if let Some(found) = pick(&serde_json::from_str(&line).unwrap()) {
            return found;
        }
    }
}

#[test]
fn an_unlisted_codex_thread_is_attached_with_codex_resume() {
    let fixture = start("attach");
    let (mut reader, mut writer) = connect(&fixture.dir.join("agentd.sock"));
    let mut send = |line: Value| writeln!(writer, "{line}").unwrap();
    send(json!({"id": 1, "method": "hello", "params": {"protocol": 1, "client": "test"}}));
    send(json!({"id": 2, "method": "attach",
        "params": {"session": "cx:known", "cols": 80, "rows": 24}}));
    let terminal = wait_for(&mut reader, |m| {
        (m["id"] == 2).then(|| m["result"]["terminal"].as_str().unwrap().to_string())
    });
    let mut seen = String::new();
    wait_for(&mut reader, |m| {
        if m["event"] == "output" && m["params"]["terminal"] == terminal.as_str() {
            let data = m["params"]["data"].as_str().unwrap();
            let bytes = decode_bytes(data).unwrap();
            seen.push_str(&String::from_utf8_lossy(&bytes));
        }
        seen.contains("resume --remote unix:// known").then_some(())
    });

    send(json!({"id": 3, "method": "attach",
        "params": {"session": "cx:missing", "cols": 80, "rows": 24}}));
    let code = wait_for(&mut reader, |m| {
        (m["id"] == 3).then(|| m["error"]["code"].as_str().unwrap_or("").to_string())
    });
    assert_eq!(code, "unknown_session");
}

#[test]
fn a_read_cut_by_the_app_server_is_codex_unavailable() {
    let fixture = start("cut");
    let (mut reader, mut writer) = connect(&fixture.dir.join("agentd.sock"));
    let mut send = |line: Value| writeln!(writer, "{line}").unwrap();
    send(json!({"id": 1, "method": "hello", "params": {"protocol": 1, "client": "test"}}));
    send(json!({"id": 2, "method": "transcript", "params": {"session": "cx:cut"}}));
    let code = wait_for(&mut reader, |m| {
        (m["id"] == 2).then(|| m["error"]["code"].as_str().unwrap_or("").to_string())
    });
    assert_eq!(code, "codex_unavailable");
}
