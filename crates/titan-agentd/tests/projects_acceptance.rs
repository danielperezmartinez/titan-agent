#![cfg(unix)]
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Fixture {
    dir: PathBuf,
    child: Option<Child>,
}
impl Fixture {
    fn prepare(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("titan-projects-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("seed/.claude/worktrees/branch/sub")).unwrap();
        std::fs::write(dir.join("agents.json"), json!([{
            "id":"ab12cd34","pid":1,"cwd":dir.join("seed/.claude/worktrees/branch/sub"),
            "kind":"background","startedAt":1700000000000_i64,
            "sessionId":"ab12cd34-0000-4000-8000-000000000001","name":"fixture: kite","status":"busy"
        }]).to_string()).unwrap();
        for (name, body) in [
            (
                "claude",
                "#!/bin/sh\ncase \"$1\" in\n agents) cat \"$FAKE_DIR/agents.json\" ;;\nesac\n",
            ),
            ("codex", "#!/bin/sh\necho '{\"status\":\"stopped\"}'\n"),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { dir, child: None }
    }
    fn file(&self) -> PathBuf {
        self.dir.join("config/projects.json")
    }
    fn start(&mut self) {
        self.child = Some(
            Command::new(env!("CARGO_BIN_EXE_titan-agentd"))
                .env("TITAN_AGENTD_SOCKET", self.dir.join("run/agentd.sock"))
                .env("TITAN_AGENTD_CLAUDE", self.dir.join("claude"))
                .env("TITAN_AGENTD_CODEX", self.dir.join("codex"))
                .env("TITAN_AGENTD_POLL_MS", "200")
                .env("TITAN_AGENT_CONFIG_DIR", self.dir.join("config"))
                .env("HOME", &self.dir)
                .env("FAKE_DIR", &self.dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
    }
    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    fn connect(&self) -> Connection {
        let deadline = Instant::now() + Duration::from_secs(10);
        let stream = loop {
            if let Ok(stream) = UnixStream::connect(self.dir.join("run/agentd.sock")) {
                break stream;
            }
            assert!(Instant::now() < deadline, "socket did not become ready");
            std::thread::sleep(Duration::from_millis(20));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut connection = Connection {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
            pending: vec![],
        };
        connection.send(json!({"id":1,"method":"hello","params":{"protocol":1,"client":"test"}}));
        assert_eq!(connection.until(|m| m["id"] == 1)["result"]["protocol"], 1);
        connection.send(json!({"id":2,"method":"subscribe"}));
        assert_eq!(connection.until(|m| m["id"] == 2)["result"], json!({}));
        connection
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
struct Connection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    pending: Vec<Value>,
}
impl Connection {
    fn send(&mut self, value: Value) {
        writeln!(self.writer, "{value}").unwrap();
    }
    fn until(&mut self, predicate: impl Fn(&Value) -> bool) -> Value {
        if let Some(index) = self.pending.iter().position(&predicate) {
            return self.pending.remove(index);
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("message deadline exceeded");
            self.reader
                .get_ref()
                .set_read_timeout(Some(remaining))
                .unwrap();
            let mut line = String::new();
            self.reader
                .read_line(&mut line)
                .expect("message must arrive within ten seconds");
            assert!(!line.is_empty(), "daemon closed connection");
            let value: Value = serde_json::from_str(&line).unwrap();
            if predicate(&value) {
                return value;
            }
            self.pending.push(value);
        }
    }
    fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"id":id,"method":method,"params":params}));
        self.until(|m| m["id"] == id)
    }
    fn projects(&mut self, predicate: impl Fn(&[Value]) -> bool) -> Value {
        self.until(|m| {
            m["event"] == "sessions"
                && m["params"]["projects"]
                    .as_array()
                    .is_some_and(|p| predicate(p))
        })
    }
}
fn assert_error(message: Value, code: &str) {
    assert_eq!(message["error"]["code"], code);
    assert!(
        !message["error"]["message"]
            .as_str()
            .unwrap()
            .trim()
            .is_empty()
    );
}

#[test]
fn first_import_is_shared_mutable_persistent_and_marks_missing_folders() {
    let mut fixture = Fixture::prepare("lifecycle");
    assert!(!fixture.file().exists());
    fixture.start();
    let seed = fixture.dir.join("seed").to_str().unwrap().to_owned();
    let added = fixture.dir.join("added").to_str().unwrap().to_owned();
    std::fs::create_dir_all(&added).unwrap();
    let mut first = fixture.connect();
    let initial = first.projects(|p| p.len() == 1 && p[0]["root"] == seed);
    assert!(initial["params"]["projects_error"].is_null());
    assert_eq!(initial["params"]["projects"][0]["name"], "seed");
    assert_eq!(initial["params"]["projects"][0]["missing"], false);
    assert!(initial["params"]["projects"][0]["added_ms"].is_i64());
    // One source succeeds and the other errors: both have still reported.
    let sources = initial["params"]["sources"].as_array().unwrap();
    assert!(
        sources
            .iter()
            .any(|v| v["agent"] == "codex" && v["ok"] == false)
    );
    assert!(
        sources
            .iter()
            .any(|v| v["agent"] == "claude_code" && v["ok"] == true)
    );
    let initial_disk: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture.file()).unwrap()).unwrap();
    assert_eq!(initial_disk["version"], 1);
    assert_eq!(initial_disk["projects"][0]["root"], seed);
    let mut second = fixture.connect();
    second.projects(|p| p.len() == 1 && p[0]["root"] == seed);
    assert_eq!(
        first.call(3, "project_add", json!({"path":added}))["result"],
        json!({"root":added})
    );
    let changed = first.projects(|p| p.len() == 2 && p[0]["root"] == seed && p[1]["root"] == added);
    let shared = second.projects(|p| p.len() == 2 && p[1]["root"] == added);
    assert_eq!(changed["params"]["projects"], shared["params"]["projects"]);
    let disk_after_add = std::fs::read_to_string(fixture.file()).unwrap();
    assert_eq!(
        first.call(4, "project_add", json!({"path":format!("{added}///")}))["result"],
        json!({"root":added})
    );
    let repeated: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture.file()).unwrap()).unwrap();
    assert_eq!(
        repeated,
        serde_json::from_str::<Value>(&disk_after_add).unwrap()
    );
    assert_eq!(
        first.call(5, "project_remove", json!({"root":format!("{seed}///")}))["result"],
        json!({})
    );
    first.projects(|p| p.len() == 1 && p[0]["root"] == added);
    second.projects(|p| p.len() == 1 && p[0]["root"] == added);
    assert!(
        fixture
            .dir
            .join("seed/.claude/worktrees/branch/sub")
            .is_dir()
    );
    assert_eq!(
        first.call(6, "project_remove", json!({"root":seed}))["result"],
        json!({})
    );
    let plain_file = fixture.dir.join("plain-file");
    std::fs::write(&plain_file, "fixture").unwrap();
    for (id, path) in [
        (7, json!("relative/folder")),
        (8, json!(fixture.dir.join("absent"))),
        (9, json!(plain_file)),
    ] {
        assert_error(
            first.call(id, "project_add", json!({"path":path})),
            "not_a_folder",
        );
    }
    let persisted: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture.file()).unwrap()).unwrap();
    assert_eq!(persisted["projects"].as_array().unwrap().len(), 1);
    assert_eq!(persisted["projects"][0]["root"], added);
    fixture.stop();
    drop(first);
    drop(second);
    std::fs::remove_dir_all(&added).unwrap();
    fixture.start();
    let mut restarted = fixture.connect();
    let snapshot =
        restarted.projects(|p| p.len() == 1 && p[0]["root"] == added && p[0]["missing"] == true);
    assert_eq!(
        snapshot["params"]["projects"][0]["added_ms"],
        persisted["projects"][0]["added_ms"]
    );
    assert!(snapshot["params"]["projects_error"].is_null());
}

#[test]
fn unknown_file_version_is_reported_and_never_overwritten() {
    let mut fixture = Fixture::prepare("unreadable");
    std::fs::create_dir_all(fixture.dir.join("config")).unwrap();
    let original =
        "{\"version\":42,\"projects\":[{\"root\":\"/fixture/retained\",\"added_ms\":7}]}\n";
    std::fs::write(fixture.file(), original).unwrap();
    fixture.start();
    let mut client = fixture.connect();
    let snapshot = client.until(|m| {
        m["event"] == "sessions"
            && m["params"]["sessions"]
                .as_array()
                .is_some_and(|s| !s.is_empty())
            && m["params"]["sources"]
                .as_array()
                .is_some_and(|s| s.len() == 2)
    });
    assert_eq!(snapshot["params"]["projects"], json!([]));
    assert!(
        !snapshot["params"]["projects_error"]
            .as_str()
            .unwrap()
            .trim()
            .is_empty()
    );
    assert_eq!(std::fs::read_to_string(fixture.file()).unwrap(), original);
    assert_error(
        client.call(3, "project_add", json!({"path":fixture.dir.join("seed")})),
        "config_error",
    );
    assert_error(
        client.call(4, "project_remove", json!({"root":"/fixture/retained"})),
        "config_error",
    );
    assert_eq!(std::fs::read_to_string(fixture.file()).unwrap(), original);
    let mut observer = fixture.connect();
    let snapshot = observer.projects(|p| p.is_empty());
    assert!(
        !snapshot["params"]["projects_error"]
            .as_str()
            .unwrap()
            .trim()
            .is_empty()
    );
    assert_eq!(std::fs::read_to_string(fixture.file()).unwrap(), original);
}
