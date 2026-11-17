//! End-to-end test of the updater (ADR-0011 §6–§7), without the real
//! repository: a daemon "installed" in a temporary prefix finds a newer
//! version on a local HTTP server, signed with a key generated for this
//! test, prepares it, waits while a working session has a terminal here,
//! and on `update_apply {force}` replaces itself with `exec`. A debug build
//! is what lets the URL and the key be changed (a release build ignores
//! those variables). Also: a tampered archive is never extracted.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const AGENTS: &str = r#"[{"id":"ab12cd34","pid":1,"cwd":"/tmp","kind":"background",
  "startedAt":1700000000000,"sessionId":"ab12cd34-0000-4000-8000-000000000001",
  "name":"demo: kite","status":"busy"}]"#;

const FAKE_CLAUDE: &str = r#"#!/bin/sh
case "$1" in
  agents) cat "$FAKE_DIR/agents.json" ;;
  attach) printf 'attached\r\n'; sleep 600 ;;
esac
"#;

const SESSION: &str = "cl:ab12cd34-0000-4000-8000-000000000001";

const FAKE_CODEX: &str = "#!/bin/sh\necho '{\"status\":\"stopped\"}'\n";

fn script(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "timed out: {what}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A tar.gz of version `v` with the real daemon and three fake binaries.
fn archive(v: &str, daemon: &Path) -> Vec<u8> {
    let root = format!("titan-agent-{v}-linux-x86_64");
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(gz);
    tar.append_path_with_name(daemon, format!("{root}/bin/titan-agentd"))
        .unwrap();
    for name in ["titan-agent", "titan-team", "titan-agent-launcher"] {
        let body = format!("#!/bin/sh\necho {name} {v}\n");
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, format!("{root}/bin/{name}"), body.as_bytes())
            .unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}

/// Serves `files` (path → bytes) over HTTP/1.0 until the test ends, and
/// keeps the head of every request in `seen`.
fn serve(files: Vec<(String, Vec<u8>)>, seen: Arc<Mutex<Vec<String>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/releases", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                request.push(byte[0]);
            }
            let line = String::from_utf8_lossy(&request);
            seen.lock().unwrap().push(line.to_string());
            let path = line.split(' ').nth(1).unwrap_or("").to_string();
            let found = files.iter().find(|(p, _)| *p == path);
            let (status, body) = match found {
                Some((_, body)) => ("200 OK", body.clone()),
                None => ("404 Not Found", Vec::new()),
            };
            let _ = write!(
                stream,
                "HTTP/1.0 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(&body);
        }
    });
    base
}

struct Install {
    dir: PathBuf,
    root: PathBuf,
    socket: PathBuf,
    daemon: Option<Child>,
    /// Heads of the HTTP requests the daemon made.
    seen: Arc<Mutex<Vec<String>>>,
}

impl Install {
    /// Installs the real daemon as version 0.6.0 and publishes 0.6.1 (with
    /// `tamper`, an archive whose hash does not match).
    fn new(name: &str, tamper: bool) -> Install {
        let dir =
            std::env::temp_dir().join(format!("titan-update-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("data/titan-agent");
        let real = Path::new(env!("CARGO_BIN_EXE_titan-agentd"));
        let bin = root.join("versions/0.6.0/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::copy(real, bin.join("titan-agentd")).unwrap();
        for name in ["titan-agent", "titan-team", "titan-agent-launcher"] {
            script(&bin.join(name), "#!/bin/sh\n");
        }
        std::os::unix::fs::symlink("versions/0.6.0", root.join("current")).unwrap();
        std::fs::write(dir.join("agents.json"), AGENTS).unwrap();
        script(&dir.join("claude"), FAKE_CLAUDE);
        script(&dir.join("codex"), FAKE_CODEX);

        let key = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let tarball = archive("0.6.1", real);
        let name = "titan-agent-0.6.1-linux-x86_64.tar.gz";
        let sha = titan_agent_core::update::sha256_hex(&tarball);
        let manifest = serde_json::to_vec(&json!({
            "format": 1, "version": "0.6.1", "published": "2026-11-20T10:00:00Z",
            "notes_url": "https://example.invalid/notes/0.6.1",
            "artifacts": [{"target": "linux-x86_64", "name": name, "size": tarball.len(),
                "sha256": if tamper { "0".repeat(64) } else { sha }}]
        }))
        .unwrap();
        let signature = minisign::sign(
            Some(&key.pk),
            &key.sk,
            std::io::Cursor::new(&manifest),
            Some(&titan_agent_core::update::trusted_comment("0.6.1")),
            None,
        )
        .unwrap()
        .into_string();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let base = serve(
            vec![
                (
                    "/releases/latest/download/titan-agent-update.json".into(),
                    manifest,
                ),
                (
                    "/releases/latest/download/titan-agent-update.json.minisig".into(),
                    signature.into_bytes(),
                ),
                (format!("/releases/download/v0.6.1/{name}"), tarball),
            ],
            Arc::clone(&seen),
        );
        let socket = dir.join("run/agentd.sock");
        let mut install = Install {
            dir,
            root,
            socket,
            daemon: None,
            seen,
        };
        install.start(&base, &key.pk.to_base64());
        install
    }

    fn start(&mut self, base: &str, key: &str) {
        let daemon = Command::new(self.root.join("versions/0.6.0/bin/titan-agentd"))
            .env("TITAN_AGENTD_SOCKET", &self.socket)
            .env("TITAN_AGENTD_CLAUDE", self.dir.join("claude"))
            .env("TITAN_AGENTD_CODEX", self.dir.join("codex"))
            .env("TITAN_AGENTD_POLL_MS", "200")
            .env("TITAN_AGENT_CONFIG_DIR", self.dir.join("config"))
            .env("TITAN_AGENT_UPDATE_URL", base)
            .env("TITAN_AGENT_UPDATE_KEYS", key)
            .env("TITAN_AGENT_UPDATE_FIRST_CHECK_S", "0")
            .env_remove("TITAN_AGENT_NO_UPDATE")
            .env("HOME", &self.dir)
            .env("FAKE_DIR", &self.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(self.dir.join("daemon.log")).unwrap())
            .spawn()
            .unwrap();
        self.daemon = Some(daemon);
        let socket = self.socket.clone();
        wait_for("socket", || UnixStream::connect(&socket).is_ok());
    }

    fn connect(&self) -> Connection {
        let stream = UnixStream::connect(&self.socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        let mut c = Connection {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
            next_id: 10,
        };
        c.request("hello", json!({"protocol": 1, "client": "e2e"}));
        c.request("subscribe", Value::Null);
        c
    }
}

impl Drop for Install {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let log = std::fs::read_to_string(self.dir.join("daemon.log")).unwrap_or_default();
            eprintln!("--- daemon.log ---\n{log}");
        }
        if let Some(mut daemon) = self.daemon.take() {
            let _ = daemon.kill();
            let _ = daemon.wait();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Connection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
}

impl Connection {
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let mut message = json!({"id": id, "method": method});
        if !params.is_null() {
            message["params"] = params;
        }
        writeln!(self.writer, "{message}").unwrap();
        self.until(|m| m["id"] == id)
    }

    fn until(&mut self, mut matches: impl FnMut(&Value) -> bool) -> Value {
        loop {
            let mut line = String::new();
            assert_ne!(
                self.reader.read_line(&mut line).unwrap(),
                0,
                "connection closed"
            );
            let message: Value = serde_json::from_str(&line).unwrap();
            if matches(&message) {
                return message;
            }
        }
    }

    fn update_state(&mut self, state: &str) -> Value {
        self.until(|m| m["event"] == "update" && m["params"]["state"] == state)
    }
}

#[test]
fn a_signed_version_is_prepared_waits_for_working_terminals_and_applies_with_exec() {
    let install = Install::new("apply", false);
    let mut window = install.connect();
    let ready = window.update_state("ready");
    assert_eq!(ready["params"]["version"], "0.6.1");
    assert_eq!(
        ready["params"]["notes_url"],
        "https://example.invalid/notes/0.6.1"
    );
    assert_eq!(ready["params"]["active"], true);
    let prepared = install.root.join("versions/0.6.1/bin");
    assert!(prepared.join("titan-agentd").is_file());
    assert_eq!(
        std::fs::read_link(install.root.join("current")).unwrap(),
        Path::new("versions/0.6.0"),
        "preparing does not switch current"
    );

    // A working session with a terminal here blocks applying.
    // (subscribe again: the snapshot comes at once)
    wait_for("the working session", || {
        window.request("subscribe", Value::Null);
        let snapshot = window.until(|m| m["event"] == "sessions");
        snapshot["params"]["sessions"]
            .as_array()
            .is_some_and(|s| s.iter().any(|s| s["key"] == SESSION))
    });
    let attached = window.request(
        "attach",
        json!({"session": SESSION, "cols": 80, "rows": 24}),
    );
    assert!(attached["result"]["terminal"].is_string(), "{attached}");
    let status = window.request("update_status", Value::Null);
    assert_eq!(status["result"]["blocking"], json!([SESSION]));
    let refused = window.request("update_apply", json!({"force": false}));
    assert_eq!(refused["error"]["code"], "confirm_needed", "{refused}");

    // Closing the window does not apply: the session is still working.
    drop(window);
    std::thread::sleep(Duration::from_millis(800));
    let mut window = install.connect();
    let status = window.request("update_status", Value::Null);
    assert_eq!(status["result"]["state"], "waiting", "{status}");
    assert!(!install.root.join("update/pending.json").exists());

    // «actualizar» confirmed: the daemon applies and becomes the new one.
    let pid = install.daemon.as_ref().unwrap().id();
    let applied = window.request("update_apply", json!({"force": true}));
    assert!(applied["result"].is_object(), "{applied}");
    // (the `applying` event may be lost: the socket closes at the exec)
    drop(window);
    let root = install.root.clone();
    wait_for("current switched", || {
        std::fs::read_link(root.join("current")).ok().as_deref()
            == Some(Path::new("versions/0.6.1"))
    });
    let pending: Value =
        serde_json::from_slice(&std::fs::read(install.root.join("update/pending.json")).unwrap())
            .unwrap();
    assert_eq!(
        pending,
        json!({"from": "0.6.0", "to": "0.6.1", "attempts": 0})
    );
    assert_eq!(
        std::fs::read_to_string(install.root.join("titan-agent-launcher")).unwrap(),
        "#!/bin/sh\necho titan-agent-launcher 0.6.1\n"
    );
    // Same process (exec), now running from versions/0.6.1, on the same socket.
    let exe = PathBuf::from(format!("/proc/{pid}/exe"));
    wait_for("exec", || {
        std::fs::read_link(&exe).is_ok_and(|e| e.starts_with(root.join("versions/0.6.1")))
    });
    let socket = install.socket.clone();
    wait_for("socket again", || {
        // A connection made during the exec is reset: that is "not yet".
        UnixStream::connect(&socket).is_ok_and(|mut s| {
            s.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let hello =
                json!({"id": 1, "method": "hello", "params": {"protocol": 1, "client": "e2e"}});
            let mut line = String::new();
            writeln!(s, "{hello}").is_ok()
                && BufReader::new(&s).read_line(&mut line).is_ok_and(|n| n > 0)
                && line.contains("\"result\"")
        })
    });
    let mut window = install.connect();
    let status = window.request("update_status", Value::Null);
    assert_eq!(status["result"]["current"], "0.6.1", "{status}");
    assert_eq!(
        std::fs::read_to_string(install.socket.with_file_name("agentd.pid")).unwrap(),
        pid.to_string()
    );
}

#[test]
fn an_archive_with_another_hash_is_never_extracted() {
    let install = Install::new("tamper", true);
    let mut window = install.connect();
    let failed = window.update_state("failed");
    assert_eq!(failed["params"]["version"], "0.6.1");
    assert!(
        failed["params"]["error"]
            .as_str()
            .unwrap()
            .contains("SHA-256"),
        "{failed}"
    );
    assert!(!install.root.join("versions/0.6.1").exists());
    let leftovers: Vec<_> = std::fs::read_dir(install.root.join("update"))
        .map(|dir| dir.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    // Privacy (ADR-0011 §6): only GETs of the three files, with a bare
    // User-Agent and nothing that identifies the user.
    let seen = install.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3, "{seen:?}");
    for head in &seen {
        let lower = head.to_lowercase();
        assert!(lower.starts_with("get /releases/"), "{head}");
        assert!(lower.contains("\r\nuser-agent: titan-agent\r\n"), "{head}");
        for header in ["cookie:", "authorization:"] {
            assert!(!lower.contains(header), "{head}");
        }
    }
}
