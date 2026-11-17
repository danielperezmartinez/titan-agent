//! Public NDJSON acceptance tests with fake CLIs and a local Codex WebSocket.
#![cfg(unix)]

use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::{
    fs::PermissionsExt,
    net::{UnixListener, UnixStream},
};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tungstenite::{Message, accept};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    daemon: Child,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
    calls: Arc<Mutex<Vec<Value>>>,
}

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn thread_result(id: &str, cwd: &Path, include_turns: bool) -> Value {
    let mut result = json!({"thread":{
        "id":id,"cwd":cwd,"preview":"Fixture transcript","createdAt":1700000000,
        "updatedAt":1700000000,"source":"exec","status":{"type":"notLoaded"},
        "modelProvider":"fixture","cliVersion":"0.169.0","path":null,
        "turns":[]
    }});
    if include_turns {
        result["thread"]["turns"] = json!([
            {"id":"turn-old","items":[{"type":"userMessage","content":[{"type":"text","text":"Inspect the draft"}]}]},
            {"id":"turn-new","items":[{"type":"agentMessage","text":"Draft reviewed"}]}
        ]);
    }
    result
}

fn serve(stream: UnixStream, root: PathBuf, calls: Arc<Mutex<Vec<Value>>>, stop: Arc<AtomicBool>) {
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut ws = match accept(stream) {
        Ok(ws) => ws,
        Err(_) => return,
    };
    while !stop.load(Ordering::SeqCst) {
        let message = match ws.read() {
            Ok(message) => message,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => return,
        };
        let Message::Text(text) = message else {
            continue;
        };
        let request: Value = serde_json::from_str(text.as_ref()).unwrap();
        calls.lock().unwrap().push(request.clone());
        let Some(id) = request.get("id") else {
            continue;
        };
        let method = request["method"].as_str().unwrap_or("");
        let params = &request["params"];
        let thread_id = params["threadId"].as_str().unwrap_or("");
        let result: Result<Value, &str> = match method {
            "initialize" => Ok(
                json!({"userAgent":"fixture/0.169.0","codexHome":root,"platformFamily":"unix","platformOs":"linux"}),
            ),
            "thread/loaded/list" => Ok(json!({"data":[],"nextCursor":null})),
            "thread/list" => Ok(json!({"data":[],"nextCursor":null})),
            "account/rateLimits/read" => Ok(json!({"rateLimits":null,"rateLimitsByLimitId":{}})),
            "thread/read" if thread_id == "missing-thread" => {
                Err("thread not loaded: missing-thread")
            }
            "thread/read" if thread_id == "broken-thread" && params["includeTurns"] == true => {
                Err("fixture fallback failure")
            }
            "thread/read" => Ok(thread_result(
                thread_id,
                &root.join("project"),
                params["includeTurns"] == true,
            )),
            "thread/items/list"
                if thread_id == "fallback-thread" || thread_id == "broken-thread" =>
            {
                Err("thread/items/list is not supported yet")
            }
            "thread/items/list" => Ok(json!({"data":[
                {"turnId":"turn-new","item":{"type":"agentMessage","text":"Draft reviewed"}},
                {"turnId":"turn-old","item":{"type":"userMessage","content":[{"type":"text","text":"Inspect the draft"}]}}
            ],"nextCursor":"older-page"})),
            _ => Err("unexpected method in read-only fixture"),
        };
        let response = match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(message) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32603,"message":message}})
            }
        };
        if ws.send(Message::Text(response.to_string().into())).is_err() {
            return;
        }
    }
}

impl Fixture {
    fn start(running: bool) -> Self {
        let serial = NEXT_FIXTURE.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("tta-transcript-{}-{serial}", std::process::id()));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("project")).unwrap();
        let listener = UnixListener::bind(root.join("codex.sock")).unwrap();
        listener.set_nonblocking(true).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (server_root, server_calls, server_stop) = (root.clone(), calls.clone(), stop.clone());
        let server = thread::spawn(move || {
            let mut workers = Vec::new();
            while !server_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (root, calls, stop) = (
                            server_root.clone(),
                            server_calls.clone(),
                            server_stop.clone(),
                        );
                        workers.push(thread::spawn(move || serve(stream, root, calls, stop)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(e) => panic!("fake listener: {e}"),
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        let version = if running {
            json!({"status":"running","socketPath":root.join("codex.sock")})
        } else {
            json!({"status":"stopped"})
        };
        fs::write(root.join("version.json"), version.to_string()).unwrap();
        executable(
            &root.join("claude"),
            "#!/bin/sh\nif [ \"$1\" = agents ]; then echo '[]'; fi\n",
        );
        executable(
            &root.join("codex"),
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$FIXTURE_ROOT/cli-calls"
if [ "$1" = app-server ] && [ "$2" = daemon ] && [ "$3" = version ]; then
    cat "$FIXTURE_ROOT/version.json"
else
    printf '%s\n' "$*" >> "$FIXTURE_ROOT/unexpected-cli"
    exit 1
fi
"#,
        );
        let daemon = Command::new(env!("CARGO_BIN_EXE_titan-agentd"))
            .env("TITAN_AGENTD_SOCKET", root.join("agentd.sock"))
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
        Self {
            root,
            daemon,
            stop,
            server: Some(server),
            calls,
        }
    }

    fn connect(&self) -> Connection {
        let deadline = Instant::now() + Duration::from_secs(10);
        let stream = loop {
            if let Ok(stream) = UnixStream::connect(self.root.join("agentd.sock")) {
                break stream;
            }
            assert!(Instant::now() < deadline, "daemon socket deadline exceeded");
            thread::sleep(Duration::from_millis(20));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut c = Connection {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
        };
        let hello = c.request(1, "hello", json!({"protocol":1,"client":"acceptance"}));
        assert_eq!(hello["result"]["protocol"], 1);
        c
    }

    fn assert_read_only(&self) {
        assert!(
            !self.root.join("unexpected-cli").exists(),
            "transcript must never launch a writer CLI"
        );
        for call in self.calls.lock().unwrap().iter() {
            let method = call["method"].as_str().unwrap();
            assert!(
                matches!(
                    method,
                    "initialize"
                        | "initialized"
                        | "thread/loaded/list"
                        | "thread/list"
                        | "thread/read"
                        | "thread/items/list"
                        | "account/rateLimits/read"
                ),
                "unexpected RPC: {call}"
            );
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        self.stop.store(true, Ordering::SeqCst);
        if let Some(server) = self.server.take() {
            server.join().unwrap();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Connection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}
impl Connection {
    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        writeln!(
            self.writer,
            "{}",
            json!({"id":id,"method":method,"params":params})
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(
                Instant::now() < deadline,
                "response deadline exceeded for {method}"
            );
            let mut line = String::new();
            assert_ne!(
                self.reader.read_line(&mut line).unwrap(),
                0,
                "daemon disconnected"
            );
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
        }
    }
}

#[test]
fn unlisted_threads_are_read_chronologically_without_opening_a_writer_including_fallback() {
    let fixture = Fixture::start(true);
    let mut c = fixture.connect();
    // Both discovery RPCs always return empty lists: neither thread can be listed.
    for (id, name, more) in [(7, "fixture-thread", true), (8, "fallback-thread", false)] {
        let session = format!("cx:{name}");
        let answer = c.request(id, "transcript", json!({"session":session}));
        assert_eq!(
            answer,
            json!({"id":id,"result":{
                "session":session,"cwd":fixture.root.join("project"),"more":more,
                "items":[
                    {"turn":"turn-old","kind":"user","text":"Inspect the draft"},
                    {"turn":"turn-new","kind":"agent","text":"Draft reviewed"}
                ]
            }})
        );
        let calls = fixture.calls.lock().unwrap();
        let thread_calls: Vec<_> = calls
            .iter()
            .filter(|r| r["params"]["threadId"] == name)
            .collect();
        let read = thread_calls
            .iter()
            .position(|r| r["method"] == "thread/read")
            .expect("thread existence must be checked");
        assert_eq!(thread_calls[read]["params"], json!({"threadId":name}));
        let list = thread_calls
            .iter()
            .position(|r| r["method"] == "thread/items/list")
            .expect("items page must be requested");
        assert!(read < list);
        assert_eq!(
            thread_calls[list]["params"],
            json!({"threadId":name,"limit":200,"sortDirection":"desc"})
        );
        if name == "fallback-thread" {
            assert!(
                thread_calls
                    .iter()
                    .skip(list + 1)
                    .any(|r| r["method"] == "thread/read"
                        && r["params"] == json!({"threadId":name,"includeTurns":true})),
                "unsupported items list must fall back to thread/read with turns"
            );
        }
    }
    fixture.assert_read_only();
}

#[test]
fn transcript_errors_distinguish_invalid_keys_missing_threads_read_failures_and_unavailable_codex()
{
    let fixture = Fixture::start(true);
    let mut c = fixture.connect();
    for (id, session) in [
        (10, "cl:fixture-director"),
        (11, "cx:"),
        (12, "fixture-thread"),
    ] {
        let answer = c.request(id, "transcript", json!({"session":session}));
        assert_eq!(answer["error"]["code"], "not_codex");
    }
    let missing = c.request(13, "transcript", json!({"session":"cx:missing-thread"}));
    assert_eq!(missing["error"]["code"], "not_found");
    assert!(
        missing["error"]["message"]
            .as_str()
            .unwrap()
            .contains("thread not loaded: missing-thread")
    );
    let broken = c.request(14, "transcript", json!({"session":"cx:broken-thread"}));
    assert_eq!(broken["error"]["code"], "codex_error");
    fixture.assert_read_only();
    let stopped = Fixture::start(false);
    let unavailable =
        stopped
            .connect()
            .request(15, "transcript", json!({"session":"cx:fixture-thread"}));
    assert_eq!(unavailable["error"]["code"], "codex_unavailable");
    stopped.assert_read_only();
}
