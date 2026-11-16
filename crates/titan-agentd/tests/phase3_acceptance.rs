//! Snapshot acceptance test using only fake local CLIs and the public socket.
#![cfg(unix)]

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::{fs::PermissionsExt, net::UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
    daemon: Child,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn event(kind: &str, task: &str, ts: i64) -> Value {
    json!({"v":1,"ts_ms":ts,"task":task,"type":kind})
}

fn created(task: &str, ts: i64) -> Value {
    let mut e = event("task_created", task, ts);
    e.as_object_mut().unwrap().extend(json!({
        "base":"main","base_commit":"0123456789abcdef0123456789abcdef01234567",
        "branch":format!("encargo/{task}"),"worktree":format!(".agents/worktrees/{task}"),
        "team":{"name":"paper-team","director":"director","roles":[
            {"name":"director","cli":"claude","model":"fixture-model","effort":null,"sandbox":null}
        ],"steps":[],"review_role":null,"max_review_rounds":0}
    }).as_object().unwrap().clone());
    e
}

fn write_events(state: &Path, task: &str, events: &[Value]) -> PathBuf {
    let dir = state.join(task);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("events.jsonl");
    let mut bytes = String::new();
    for e in events {
        bytes.push_str(&e.to_string());
        bytes.push('\n');
    }
    fs::write(&path, bytes).unwrap();
    path
}

// Include directories as well as files: the reader must not create state either.
fn contents(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(base: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let key = path.strip_prefix(base).unwrap().to_owned();
            if path.is_dir() {
                out.insert(key, None);
                visit(base, &path, out);
            } else {
                out.insert(key, Some(fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

fn until(reader: &mut BufReader<UnixStream>, mut predicate: impl FnMut(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "snapshot deadline exceeded");
        let mut line = String::new();
        assert_ne!(
            reader.read_line(&mut line).unwrap(),
            0,
            "daemon disconnected"
        );
        let value = serde_json::from_str(&line).unwrap();
        if predicate(&value) {
            return value;
        }
    }
}

#[test]
fn snapshot_reads_recent_open_teams_limits_history_and_polls_without_writing_state() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let root = std::env::temp_dir().join(format!("ttd-accept-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let project = root.join("paper-project");
    let folder = project.join(".agents/worktrees/crew");
    let agents = folder.join(".agents");
    let state = agents.join("custom-state");
    fs::create_dir_all(&state).unwrap();
    fs::write(
        agents.join("team.toml"),
        r#"
version = 1
name = "paper-team"
state_dir = ".agents/custom-state"
[roles.director]
cli = "claude"
model = "fixture-model"
launch = false
"#,
    )
    .unwrap();
    let mut open = vec![created("paper", now)];
    for i in 0..304 {
        let mut e = event("decision", "paper", now);
        e["by"] = json!("director");
        e["text"] = json!(format!("Decision {i}"));
        e["extra_field"] = json!(i);
        open.push(e);
    }
    // The daemon forwards JSON objects even if the core would ignore their type/version.
    let mut future = event("future_event", "paper", now);
    future["v"] = json!(99);
    future["extra_field"] = json!({"keep":true});
    open.push(future);
    let open_path = write_events(&state, "paper", &open);
    let mut bytes = fs::read(&open_path).unwrap();
    bytes.extend_from_slice(b"broken JSON\n{\n");
    fs::write(&open_path, bytes).unwrap();
    let mut closed_event = event("task_closed", "closed", now);
    closed_event["delete_branch"] = json!(false);
    write_events(&state, "closed", &[created("closed", now), closed_event]);
    write_events(
        &state,
        "expired",
        &[created("expired", now - 13 * 60 * 60 * 1000)],
    );
    fs::write(root.join("agents.json"), json!([{
        "id":"ab12cd34","pid":1,"cwd":folder,"kind":"background","startedAt":now,
        "sessionId":"ab12cd34-0000-4000-8000-000000000001","name":"paper-director","status":"busy"
    }]).to_string()).unwrap();
    executable(
        &root.join("claude"),
        "#!/bin/sh\nif [ \"$1\" = agents ]; then cat \"$FIXTURE_ROOT/agents.json\"; fi\n",
    );
    executable(
        &root.join("codex"),
        "#!/bin/sh\nprintf '%s\\n' '{\"status\":\"stopped\"}'\n",
    );
    let before = contents(&agents);
    let socket = root.join("s.sock");
    let daemon = Command::new(env!("CARGO_BIN_EXE_titan-agentd"))
        .env("TITAN_AGENTD_SOCKET", &socket)
        .env("TITAN_AGENTD_CLAUDE", root.join("claude"))
        .env("TITAN_AGENTD_CODEX", root.join("codex"))
        .env("TITAN_AGENTD_POLL_MS", "100")
        .env("FIXTURE_ROOT", &root)
        .env("HOME", &root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _fixture = Fixture { root, daemon };
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        if let Ok(stream) = UnixStream::connect(&socket) {
            break stream;
        }
        assert!(Instant::now() < deadline, "daemon did not open socket");
        std::thread::sleep(Duration::from_millis(20));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    writeln!(
        stream,
        "{}",
        json!({"id":1,"method":"hello","params":{"protocol":1,"client":"acceptance"}})
    )
    .unwrap();
    assert_eq!(
        until(&mut reader, |v| v["id"] == 1)["result"]["protocol"],
        1
    );
    writeln!(stream, "{}", json!({"id":2,"method":"subscribe"})).unwrap();
    let snapshot = until(&mut reader, |v| {
        v["event"] == "sessions"
            && v["params"]["sessions"]
                .as_array()
                .is_some_and(|s| !s.is_empty())
    });
    let teams = snapshot["params"]["teams"]
        .as_array()
        .expect("teams is always present");
    assert_eq!(teams.len(), 1, "closed and expired tasks must be excluded");
    assert_eq!(teams[0]["task"], "paper");
    assert_eq!(teams[0]["folder"], json!(folder));
    assert_eq!(teams[0]["project_root"], json!(project));
    assert_eq!(teams[0]["events"], json!(&open[open.len() - 300..]));
    assert_eq!(
        snapshot["params"]["quotas"],
        json!([]),
        "failed Codex and Claude must not invent quotas"
    );
    assert_eq!(contents(&agents), before);

    let mut update = event("decision", "paper", now + 1);
    update["by"] = json!("director");
    update["text"] = json!("Recheck the outline.");
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&open_path)
        .unwrap();
    writeln!(file, "{update}").unwrap();
    let after_update = contents(&agents);
    until(&mut reader, |v| {
        v["event"] == "sessions"
            && v["params"]["teams"][0]["events"]
                .as_array()
                .is_some_and(|es| es.last() == Some(&update))
    });
    assert_eq!(contents(&agents), after_update);
    let mut close = event("task_closed", "paper", now + 2);
    close["delete_branch"] = json!(false);
    writeln!(file, "{close}").unwrap();
    let after_close = contents(&agents);
    until(&mut reader, |v| {
        v["event"] == "sessions" && v["params"]["teams"] == json!([])
    });
    assert_eq!(contents(&agents), after_close);
}
