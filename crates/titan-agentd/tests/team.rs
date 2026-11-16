//! titan-agentd with `--fixture`: the team tool's event log next to the
//! sessions reaches the snapshot as a task joined to them, live.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct Daemon {
    dir: PathBuf,
    child: Child,
    socket: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn session(key: &str, agent: &str, cwd: &str, state: &str) -> Value {
    json!({
        "key": key, "agent": agent, "cli_id": key, "name": key, "cwd": cwd,
        "project_root": cwd, "project": "repo", "kind": "background",
        "state": state, "model": "opus"
    })
}

fn event(kind: Value) -> String {
    let mut line = kind;
    line["v"] = json!(1);
    line["at_ms"] = json!(titan_agent_core::time::now_ms());
    line["task"] = json!("demo");
    line["director"] = json!("d1");
    format!("{line}\n")
}

#[test]
fn a_team_task_reaches_the_snapshot_joined_to_its_sessions() {
    let dir = std::env::temp_dir().join(format!("titan-agentd-team-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let base = dir.join("repo");
    std::fs::create_dir_all(base.join(".agents/state")).unwrap();
    std::fs::create_dir_all(base.join(".agents/worktrees/demo")).unwrap();
    std::fs::write(
        base.join(".agents/team.toml"),
        "version = 1\n[roles.director]\ncli = \"claude\"\nmodel = \"opus\"\nlaunch = false\n[roles.revisor]\ncli = \"codex\"\nmodel = \"gpt-6.1-sol\"\nprompt_file = \"p\"\n",
    )
    .unwrap();
    let base_text = base.display().to_string();
    let worktree = format!("{base_text}/.agents/worktrees/demo");
    let fixture = dir.join("fixture.json");
    std::fs::write(
        &fixture,
        json!({
            "sessions": [
                session("cl:d1", "claude_code", &base_text, "working"),
                session("cx:th-1", "codex", &worktree, "working"),
            ],
            "quotas": [{"agent": "codex", "limit": "codex", "window_mins": 300, "used_percent": 75.0}]
        })
        .to_string(),
    )
    .unwrap();
    let log = base.join(".agents/state/events.jsonl");
    let mut file = std::fs::File::create(&log).unwrap();
    file.write_all(
        event(
            json!({"type": "task_created", "branch": "encargo/demo", "base": "main",
            "base_dir": base_text, "roles": [
                {"name": "director", "cli": "claude", "model": "opus", "launch": false},
                {"name": "revisor", "cli": "codex", "model": "gpt-6.1-sol", "launch": true}],
            "steps": ["revisor"], "review_role": "revisor", "max_review_rounds": 1}),
        )
        .as_bytes(),
    )
    .unwrap();

    let socket = dir.join("run/agentd.sock");
    let child = Command::new(env!("CARGO_BIN_EXE_titan-agentd"))
        .args(["--fixture", fixture.to_str().unwrap()])
        .env("TITAN_AGENTD_SOCKET", &socket)
        .env("TITAN_AGENTD_POLL_MS", "200")
        .env("HOME", &dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let daemon = Daemon {
        dir: dir.clone(),
        child,
        socket,
    };
    let start = Instant::now();
    while !daemon.socket.exists() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(20));
    }
    let stream = UnixStream::connect(&daemon.socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    for line in [
        json!({"id": 1, "method": "hello", "params": {"protocol": 1, "client": "test"}}),
        json!({"id": 2, "method": "subscribe"}),
    ] {
        writer.write_all(format!("{line}\n").as_bytes()).unwrap();
    }
    let mut until = |check: &dyn Fn(&Value) -> bool| loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "the daemon closed the connection");
        let value: Value = serde_json::from_str(&line).unwrap();
        if value["event"] == "sessions" && check(&value["params"]) {
            return value["params"].clone();
        }
    };

    let snapshot = until(&|p| p["teams"].as_array().is_some_and(|t| !t.is_empty()));
    let team = &snapshot["teams"][0];
    assert_eq!(team["task"], "demo");
    assert_eq!(team["director_session"], "cl:d1");
    assert_eq!(team["max_review_rounds"], 1);
    assert_eq!(snapshot["quotas"][0]["used_percent"], 75.0);

    // A turn appended to the log shows up, joined to the role's thread.
    file.write_all(
        event(
            json!({"type": "turn_started", "role": "revisor", "turn": 1, "resumed": false,
            "round": 1, "message": "Revisa"}),
        )
        .as_bytes(),
    )
    .unwrap();
    file.write_all(
        event(
            json!({"type": "turn_finished", "role": "revisor", "turn": 1, "status": "ok",
            "thread_id": "th-1", "duration_ms": 5, "usage": {"input_tokens": 9, "output_tokens": 1},
            "summary": "ok · 0 hallazgos"}),
        )
        .as_bytes(),
    )
    .unwrap();
    let snapshot = until(&|p| {
        p["teams"][0]["events"]
            .as_array()
            .is_some_and(|e| e.len() == 3)
    });
    let team = &snapshot["teams"][0];
    assert_eq!(team["sessions"]["revisor"], "cx:th-1");
    assert_eq!(team["review_round"], 1);
    assert_eq!(team["stats"]["revisor"]["turns"], 1);
}
