//! An imported project without sessions is also a folder where team tasks
//! are looked for (ADR-0014, extends ADR-0010 §4.2): its open task reaches
//! the clients in `teams`.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

#[test]
fn an_imported_project_without_sessions_shows_its_team() {
    let dir = std::env::temp_dir().join(format!("titan-projects-teams-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = dir.join("solo");
    let state = project.join(".agents/state/recetas");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::write(
        project.join(".agents/team.toml"),
        "version = 1\nname = \"solo\"\nstate_dir = \".agents/state\"\n",
    )
    .unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let created = json!({"v":1,"ts_ms":now,"task":"recetas","type":"task_created",
        "base":"main","base_commit":"c","branch":"b","worktree":"w",
        "team":{"name":"solo","director":"director","roles":[
            {"name":"director","cli":"claude","model":"opus","effort":null,"sandbox":null}],
            "steps":[],"review_role":null,"max_review_rounds":0}});
    std::fs::write(state.join("events.jsonl"), format!("{created}\n")).unwrap();
    let root = project.to_string_lossy().to_string();
    std::fs::write(
        dir.join("config/projects.json"),
        json!({"version":1,"projects":[{"root":root,"added_ms":1}]}).to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("agents.json"), "[]").unwrap();
    for (name, body) in [
        (
            "claude",
            "#!/bin/sh\ncase \"$1\" in agents) cat \"$FAKE_DIR/agents.json\" ;; esac\n",
        ),
        ("codex", "#!/bin/sh\necho '{\"status\":\"stopped\"}'\n"),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let socket = dir.join("run/agentd.sock");
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_titan-agentd"))
        .env("TITAN_AGENTD_SOCKET", &socket)
        .env("TITAN_AGENTD_CLAUDE", dir.join("claude"))
        .env("TITAN_AGENTD_CODEX", dir.join("codex"))
        .env("TITAN_AGENTD_POLL_MS", "200")
        .env("HOME", &dir)
        .env("FAKE_DIR", &dir)
        .env("TITAN_AGENT_CONFIG_DIR", dir.join("config"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !socket.exists() {
        assert!(start.elapsed() < Duration::from_secs(10), "no socket");
        std::thread::sleep(Duration::from_millis(20));
    }
    let stream = UnixStream::connect(&socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    for line in [
        json!({"id":1,"method":"hello","params":{"protocol":1,"client":"test"}}),
        json!({"id":2,"method":"subscribe"}),
    ] {
        writer.write_all(format!("{line}\n").as_bytes()).unwrap();
    }
    let found = loop {
        assert!(start.elapsed() < Duration::from_secs(10), "no team");
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let message: Value = serde_json::from_str(&line).unwrap();
        if message["event"] == "sessions"
            && let Some(teams) = message["params"]["teams"].as_array()
            && let Some(team) = teams.iter().find(|t| t["task"] == "recetas")
        {
            break team.clone();
        }
    };
    assert_eq!(found["project_root"], root.as_str());
    let _ = daemon.kill();
    let _ = daemon.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
