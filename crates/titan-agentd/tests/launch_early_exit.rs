//! A Codex TUI that ends right after starting is a failed launch, answered
//! with what it printed (fake CLIs only).
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use titan_agent_core::launch::LaunchSpec;
use titan_agent_core::session::AgentCli;

const CODEX: &str = r#"#!/bin/sh
if [ "$1" = app-server ]; then echo '{"status":"stopped"}'; exit 0; fi
printf '\033[31merror:\033[0m unknown model example\r\n'
exit 3
"#;

#[test]
fn codex_tui_that_exits_at_once_is_a_failed_launch() {
    let dir = std::env::temp_dir().join(format!("ta-early-exit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("work")).unwrap();
    std::fs::write(dir.join("agents.json"), "[]").unwrap();
    for (name, body) in [("codex", CODEX), ("claude", "#!/bin/sh\necho '[]'\n")] {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let socket = dir.join("agentd.sock");
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_titan-agentd"))
        .env("TITAN_AGENTD_SOCKET", &socket)
        .env("TITAN_AGENTD_CLAUDE", dir.join("claude"))
        .env("TITAN_AGENTD_CODEX", dir.join("codex"))
        .env("HOME", &dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !socket.exists() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut writer = UnixStream::connect(&socket).unwrap();
    writer
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut reader = BufReader::new(writer.try_clone().unwrap());
    let spec = LaunchSpec {
        agent: AgentCli::Codex,
        cwd: dir.join("work").to_string_lossy().into_owned(),
        model: "gpt-6-luna".into(),
        effort: "low".into(),
        permission: "read-only".into(),
        brief: "hola".into(),
    };
    for line in [
        json!({"id": 1, "method": "hello", "params": {"protocol": 1, "client": "test"}}),
        json!({"id": 2, "method": "launch", "params": {
            "token": "t", "command": spec.command_line(), "spec": spec}}),
    ] {
        writeln!(writer, "{line}").unwrap();
    }
    let answer = loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        let message: Value = serde_json::from_str(&line).unwrap();
        if message["id"] == 2 {
            break message;
        }
    };
    let _ = daemon.kill();
    let _ = daemon.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(answer["error"]["code"], "launch_failed", "{answer}");
    let text = answer["error"]["message"].as_str().unwrap();
    assert!(text.contains("error: unknown model example"), "{text}");
    assert!(!text.contains('\u{1b}'), "{text}");
}
