#![cfg(unix)]

use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Fixture {
    dir: PathBuf,
    socket: PathBuf,
    daemon: Option<Child>,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("titan-update-daemon-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        for subdir in ["home", "config", "runtime", "run"] {
            fs::create_dir(dir.join(subdir)).unwrap();
        }
        for (name, body) in [
            (
                "claude",
                "#!/bin/sh\ncase \"$1\" in agents) printf '[]\\n';; esac\n",
            ),
            ("codex", "#!/bin/sh\nprintf '{\"status\":\"stopped\"}\\n'\n"),
        ] {
            let path = dir.join(name);
            fs::write(&path, body).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let socket = dir.join("run/custom-update.sock");
        Self {
            dir,
            socket,
            daemon: None,
        }
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_titan-agentd"));
        cmd.env("HOME", self.dir.join("home"))
            .env("XDG_RUNTIME_DIR", self.dir.join("runtime"))
            .env("XDG_CONFIG_HOME", self.dir.join("unused-config"))
            .env("TITAN_AGENT_CONFIG_DIR", self.dir.join("config"))
            .env("TITAN_AGENTD_SOCKET", &self.socket)
            .env("TITAN_AGENTD_CLAUDE", self.dir.join("claude"))
            .env("TITAN_AGENTD_CODEX", self.dir.join("codex"))
            .env("TITAN_AGENTD_POLL_MS", "200")
            .env_remove("TITAN_AGENT_NO_UPDATE");
        cmd
    }
    fn start(&mut self) -> Connection {
        self.daemon = Some(
            self.command()
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        let stream = loop {
            if let Ok(stream) = UnixStream::connect(&self.socket) {
                break stream;
            }
            assert!(Instant::now() < deadline, "daemon did not open its socket");
            assert!(
                self.daemon.as_mut().unwrap().try_wait().unwrap().is_none(),
                "daemon exited before accepting connections"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        Connection {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
        }
    }
    fn stop(&mut self) {
        if let Some(mut child) = self.daemon.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_file(&self.socket);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.dir);
    }
}
struct Connection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}
impl Connection {
    fn request(&mut self, id: u64, method: &str, params: Option<Value>) -> Value {
        let mut request = json!({"id":id,"method":method});
        if let Some(params) = params {
            request["params"] = params;
        }
        writeln!(self.writer, "{request}").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "no response to {method}");
            let mut line = String::new();
            assert_ne!(
                self.reader.read_line(&mut line).unwrap(),
                0,
                "connection closed"
            );
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
        }
    }
    fn hello(&mut self) {
        let hello = self.request(
            1,
            "hello",
            Some(json!({"protocol":1,"client":"acceptance"})),
        );
        assert_eq!(hello["result"]["protocol"], 1);
        assert_eq!(hello["result"]["daemon"], env!("CARGO_PKG_VERSION"));
    }
}

#[test]
fn c8_development_daemon_reports_inactive_persists_auto_and_writes_pid_beside_socket() {
    let mut fixture = Fixture::new();
    let printed = fixture
        .command()
        .env_remove("TITAN_AGENTD_SOCKET")
        .arg("--print-socket")
        .output()
        .unwrap();
    assert!(printed.status.success());
    assert_eq!(
        String::from_utf8(printed.stdout).unwrap().trim_end(),
        fixture
            .dir
            .join("runtime/titan-agent/agentd-dev.sock")
            .to_str()
            .unwrap()
    );
    let overridden = fixture.command().arg("--print-socket").output().unwrap();
    assert!(overridden.status.success());
    assert_eq!(
        String::from_utf8(overridden.stdout).unwrap().trim_end(),
        fixture.socket.to_str().unwrap()
    );
    let mut client = fixture.start();
    client.hello();
    let pid_path = fixture.socket.parent().unwrap().join("agentd.pid");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !pid_path.exists() {
        assert!(Instant::now() < deadline, "missing agentd.pid");
        std::thread::sleep(Duration::from_millis(20));
    }
    let pid: u32 = fs::read_to_string(pid_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(pid, fixture.daemon.as_ref().unwrap().id());
    let expected = json!({"state":"idle","active":false,"installed":env!("CARGO_PKG_VERSION"),
        "current":null,"version":null,"notes_url":null,"blocking":[],"error":null,
        "auto":true,"last_check_ms":null,"rolled_back":null});
    let status = client.request(2, "update_status", None);
    // Check the contracted fields while allowing additive protocol fields.
    for (field, value) in expected.as_object().unwrap() {
        assert_eq!(&status["result"][field], value, "{field}");
    }
    for (id, method, params) in [
        (3, "update_check", None),
        (4, "update_apply", None),
        (5, "update_apply", Some(json!({"force":true}))),
    ] {
        assert_eq!(
            client.request(id, method, params)["error"]["code"],
            "update_inactive"
        );
    }
    let settings = client.request(6, "update_settings", Some(json!({"auto":false})));
    assert_eq!(settings["result"]["auto"], false);
    assert_eq!(settings["result"]["active"], false);
    let saved: Value =
        serde_json::from_slice(&fs::read(fixture.dir.join("config/update.json")).unwrap()).unwrap();
    assert_eq!(saved, json!({"auto":false}));
    assert_eq!(
        client.request(7, "update_status", None)["result"]["auto"],
        false
    );
    drop(client);
    fixture.stop();
    let mut client = fixture.start();
    client.hello();
    assert_eq!(
        client.request(8, "update_status", None)["result"]["auto"],
        false
    );
}
