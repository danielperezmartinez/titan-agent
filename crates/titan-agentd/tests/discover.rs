mod support;

use serde_json::{Value, json};
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{ExitStatus, Output};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::{TIMEOUT, TempDir, frame, read_client_frame, server, upgrade};
use titan_agent_core::discovery::{
    claude_session, codex_session, parse_claude_agents, parse_codex_thread, project_for,
};
use titan_agent_core::session::Session;
use titan_agentd::codex::{CodexRpc, control_socket};
use titan_agentd::discover::{CommandRunner, LiveSource, SourceConfig};
use titan_agentd::source::{SessionSource, StopError};
use titan_agentd::ws::WsError;

type Calls = Arc<Mutex<Vec<(Vec<String>, Duration)>>>;
struct FakeRunner {
    replies: Mutex<VecDeque<io::Result<Output>>>,
    calls: Calls,
}
impl CommandRunner for FakeRunner {
    fn run(&self, argv: &[String], timeout: Duration) -> io::Result<Output> {
        self.calls.lock().unwrap().push((argv.to_vec(), timeout));
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected command")
    }
}
fn output(success: bool, stdout: &str, stderr: &str) -> io::Result<Output> {
    Ok(Output {
        status: ExitStatus::from_raw(if success { 0 } else { 256 }),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    })
}
fn source(
    home: Option<PathBuf>,
    socket: Option<PathBuf>,
    replies: Vec<io::Result<Output>>,
) -> (LiveSource, Calls) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let runner = FakeRunner {
        replies: Mutex::new(replies.into()),
        calls: calls.clone(),
    };
    (
        LiveSource::new(
            SourceConfig {
                claude_home: home,
                codex_socket: socket,
                timeout: TIMEOUT,
            },
            Box::new(runner),
        ),
        calls,
    )
}
fn agents(cwd: &str) -> String {
    json!([
        {"id":"agent-example", "cwd":cwd, "kind":"background", "sessionId":"transcript-example", "name":"Example task", "status":"busy", "startedAt":1790000000123_i64},
        {"pid":731, "cwd":cwd, "kind":"interactive", "name":"Example console", "status":"idle"}
    ]).to_string()
}
fn expected_claude(input: &str, model: Option<&str>) -> Vec<Session> {
    parse_claude_agents(input)
        .unwrap()
        .iter()
        .filter_map(|agent| {
            let project = project_for(&agent.cwd, |path| {
                std::path::Path::new(path).join(".git").exists()
            });
            claude_session(
                agent,
                project,
                agent.session_id.as_ref().and(model.map(str::to_owned)),
            )
        })
        .collect()
}
fn transcript(model: &str) -> String {
    format!(
        "{}\n",
        json!({"type":"assistant", "message":{"model":model}})
    )
}
fn request(stream: &mut UnixStream) -> Value {
    let (fin, opcode, payload) = read_client_frame(stream);
    assert!(fin);
    assert_eq!(opcode, 1);
    let value: Value = serde_json::from_slice(&payload).unwrap();
    assert!(value.get("jsonrpc").is_none());
    value
}
fn send(stream: &mut UnixStream, value: Value) {
    stream
        .write_all(&frame(true, 1, value.to_string().as_bytes()))
        .unwrap();
}
fn initialize(stream: &mut UnixStream) {
    upgrade(stream);
    let init = request(stream);
    assert_eq!(
        init,
        json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"titan-agentd","version":env!("CARGO_PKG_VERSION")}}})
    );
    send(stream, json!({"id":1,"result":{}}));
    assert_eq!(request(stream), json!({"method":"initialized"}));
}

#[test]
fn control_socket_uses_explicit_codex_home_then_default_home() {
    assert_eq!(
        control_socket(
            Some(OsStr::new("/srv/fictional/codex")),
            Some(OsStr::new("/srv/fictional/profile"))
        ),
        Some(PathBuf::from(
            "/srv/fictional/codex/app-server-control/app-server-control.sock"
        ))
    );
    assert_eq!(
        control_socket(Some(OsStr::new("/srv/fictional/codex")), None),
        Some(PathBuf::from(
            "/srv/fictional/codex/app-server-control/app-server-control.sock"
        ))
    );
    assert_eq!(
        control_socket(None, Some(OsStr::new("/srv/fictional/profile"))),
        Some(PathBuf::from(
            "/srv/fictional/profile/.codex/app-server-control/app-server-control.sock"
        ))
    );
    assert_eq!(control_socket(None, None), None);
}

#[test]
fn rpc_initializes_and_skips_notifications_requests_and_unrelated_responses() {
    let (_dir, path, handle) = server(|mut stream| {
        initialize(&mut stream);
        let call = request(&mut stream);
        assert_eq!(call["method"], "thread/read");
        assert_eq!(
            call["params"],
            json!({"threadId":"thread-example","includeTurns":false})
        );
        assert_eq!(call["id"], 2);
        send(&mut stream, json!({"method":"notice","params":{}}));
        send(
            &mut stream,
            json!({"id":900,"method":"server/request","params":{}}),
        );
        send(&mut stream, json!({"id":901,"result":"unrelated"}));
        send(
            &mut stream,
            json!({"id":call["id"],"result":{"thread":{"id":"thread-example"}}}),
        );
        let next = request(&mut stream);
        assert_eq!(next["id"], 3);
        send(
            &mut stream,
            json!({"id":next["id"],"error":{"code":-32000,"message":"fictional read failed"}}),
        );
    });
    let mut rpc = CodexRpc::connect(&path, TIMEOUT).unwrap();
    assert_eq!(
        rpc.call(
            "thread/read",
            json!({"threadId":"thread-example","includeTurns":false}),
            TIMEOUT
        )
        .unwrap(),
        json!({"thread":{"id":"thread-example"}})
    );
    assert!(
        matches!(rpc.call("thread/read", json!({"threadId":"missing","includeTurns":false}), TIMEOUT), Err(WsError::Protocol(message)) if message.contains("fictional read failed"))
    );
    handle.join().unwrap();
}

#[test]
fn claude_poll_maps_sessions_and_repository_with_and_without_transcript() {
    let dir = TempDir::new();
    let repo = dir.path().join("repository");
    let cwd = repo.join("nested");
    fs::create_dir_all(&cwd).unwrap();
    fs::write(repo.join(".git"), "gitdir: fictional\n").unwrap();
    let home = dir.path().join("claude");
    let folder = home.join("projects/fictional-project");
    fs::create_dir_all(&folder).unwrap();
    let input = agents(cwd.to_str().unwrap());
    let (mut live, calls) = source(
        Some(home),
        None,
        vec![output(true, &input, ""), output(true, &input, "")],
    );
    assert_eq!(live.poll(), expected_claude(&input, None));
    fs::write(
        folder.join("transcript-example.jsonl"),
        transcript("claude-opus-5-5"),
    )
    .unwrap();
    assert_eq!(live.poll(), expected_claude(&input, Some("opus")));
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            (
                vec!["claude".into(), "agents".into(), "--json".into()],
                TIMEOUT
            );
            2
        ]
    );
}

#[test]
fn transcript_tail_is_limited_to_64_kib_and_missing_cached_paths_are_searched_again() {
    let dir = TempDir::new();
    let first = dir.path().join("projects/first");
    let second = dir.path().join("projects/second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    let path = first.join("transcript-example.jsonl");
    let input = agents("/srv/fictional/project");
    let (mut live, _) = source(
        Some(dir.path().to_path_buf()),
        None,
        (0..5).map(|_| output(true, &input, "")).collect(),
    );
    fs::write(&path, transcript("claude-opus-5-5")).unwrap();
    assert_eq!(live.poll(), expected_claude(&input, Some("opus")));
    // The only model is now outside the permitted tail window.
    fs::write(
        &path,
        format!("{}{}\n", transcript("claude-opus-5-5"), " ".repeat(65536)),
    )
    .unwrap();
    assert_eq!(live.poll(), expected_claude(&input, None));
    let content = format!(
        "{}{}\n{}",
        transcript("claude-opus-5-5"),
        "x".repeat(70000),
        transcript("claude-3-5-sonnet-20241022")
    );
    fs::write(&path, content).unwrap();
    assert_eq!(live.poll(), expected_claude(&input, Some("sonnet")));
    fs::remove_file(path).unwrap();
    fs::write(
        second.join("transcript-example.jsonl"),
        transcript("claude-opus-5-5"),
    )
    .unwrap();
    assert_eq!(live.poll(), expected_claude(&input, Some("opus")));
    fs::remove_file(second.join("transcript-example.jsonl")).unwrap();
    assert_eq!(live.poll(), expected_claude(&input, None));
}

#[test]
fn claude_errors_and_missing_codex_socket_return_empty_without_panicking() {
    let dir = TempDir::new();
    let (mut live, calls) = source(
        None,
        Some(dir.path().join("absent")),
        vec![
            output(
                false,
                &agents("/srv/fictional/project"),
                "fictional failure",
            ),
            output(true, "invalid json", ""),
            Err(io::Error::new(io::ErrorKind::TimedOut, "fictional timeout")),
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "fictional missing command",
            )),
        ],
    );
    for _ in 0..4 {
        assert!(live.poll().is_empty());
    }
    assert_eq!(calls.lock().unwrap().len(), 4);
}

#[test]
fn missing_codex_socket_preserves_claude_and_never_runs_codex() {
    let dir = TempDir::new();
    let input = agents("/srv/fictional/project");
    let (mut live, calls) = source(
        None,
        Some(dir.path().join("absent")),
        vec![output(true, &input, "")],
    );
    assert_eq!(live.poll(), expected_claude(&input, None));
    assert_eq!(
        *calls.lock().unwrap(),
        vec![(
            vec!["claude".into(), "agents".into(), "--json".into()],
            TIMEOUT
        )]
    );
}

#[test]
fn stop_uses_official_claude_command_and_trims_stderr() {
    let sessions = expected_claude(&agents("/srv/fictional/project"), None);
    let (mut live, calls) = source(
        None,
        None,
        vec![
            output(true, "", ""),
            output(false, "", " \n fictional stop failed \t\n"),
        ],
    );
    assert!(live.stop(&sessions[0]).is_ok());
    assert!(
        matches!(live.stop(&sessions[0]), Err(StopError::Failed(message)) if message == "fictional stop failed")
    );
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            (
                vec!["claude".into(), "stop".into(), "agent-example".into()],
                TIMEOUT
            );
            2
        ]
    );
}

#[test]
fn stop_rejects_interactive_claude_and_codex_without_running_commands() {
    let sessions = expected_claude(&agents("/srv/fictional/project"), None);
    let (mut live, calls) = source(None, None, vec![]);
    assert!(matches!(
        live.stop(&sessions[1]),
        Err(StopError::Unsupported(_))
    ));
    let thread = parse_codex_thread(
        &json!({"id":"thread-example","cwd":"/srv/fictional/project","source":"appServer"}),
    )
    .unwrap();
    let session = codex_session(&thread, project_for(&thread.cwd, |_| false)).unwrap();
    assert!(
        matches!(live.stop(&session), Err(StopError::Unsupported(message)) if message == "Codex no tiene comando oficial para detener un hilo")
    );
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn codex_connection_failure_preserves_claude_sessions() {
    let (_dir, path, handle) = server(|mut stream| {
        // A listening endpoint with a failed upgrade is not a running app-server.
        support::read_upgrade(&mut stream);
        stream
            .write_all(b"HTTP/1.1 503 Unavailable\r\n\r\n")
            .unwrap();
    });
    let input = agents("/srv/fictional/project");
    let (mut live, calls) = source(None, Some(path), vec![output(true, &input, "")]);
    assert_eq!(live.poll(), expected_claude(&input, None));
    assert_eq!(calls.lock().unwrap().len(), 1);
    handle.join().unwrap();
}

#[test]
fn broken_codex_connection_is_retried_only_after_ten_seconds() {
    use std::os::unix::net::UnixListener;
    use std::time::Instant;

    let dir = TempDir::new();
    let path = dir.path().join("s");
    let listener = UnixListener::bind(&path).unwrap();
    let input = agents("/srv/fictional/project");
    let (mut live, calls) = source(
        None,
        Some(path),
        (0..3).map(|_| output(true, &input, "")).collect(),
    );
    let handle = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        for attempt in 0..2 {
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "Codex was not reconnected");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            };
            stream.set_read_timeout(Some(TIMEOUT)).unwrap();
            stream.set_write_timeout(Some(TIMEOUT)).unwrap();
            initialize(&mut stream);
            let list = request(&mut stream);
            assert_eq!(list["method"], "thread/loaded/list");
            send(
                &mut stream,
                json!({"method":"fictional/notice","params":{"attempt":attempt}}),
            );
            stream.write_all(&frame(true, 8, &[])).unwrap();
        }
    });
    assert_eq!(live.poll(), expected_claude(&input, None));
    assert_eq!(live.poll(), expected_claude(&input, None));
    std::thread::sleep(Duration::from_millis(10100));
    assert_eq!(live.poll(), expected_claude(&input, None));
    drop(live);
    handle.join().unwrap();
    assert_eq!(calls.lock().unwrap().len(), 3);
}

#[test]
fn loaded_threads_propagates_list_rpc_errors() {
    let (_dir, path, handle) = server(|mut stream| {
        initialize(&mut stream);
        let list = request(&mut stream);
        assert_eq!(list["method"], "thread/loaded/list");
        send(
            &mut stream,
            json!({"id":list["id"],"error":{"code":-32000,"message":"fictional list failed"}}),
        );
    });
    let mut rpc = CodexRpc::connect(&path, TIMEOUT).unwrap();
    assert!(
        matches!(rpc.loaded_threads(TIMEOUT), Err(WsError::Protocol(message)) if message.contains("fictional list failed"))
    );
    handle.join().unwrap();
}

#[test]
fn stop_reports_command_io_failure() {
    let sessions = expected_claude(&agents("/srv/fictional/project"), None);
    let (mut live, calls) = source(
        None,
        None,
        vec![Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "fictional stop timeout",
        ))],
    );
    assert!(matches!(live.stop(&sessions[0]), Err(StopError::Failed(_))));
    assert_eq!(
        *calls.lock().unwrap(),
        vec![(
            vec!["claude".into(), "stop".into(), "agent-example".into()],
            TIMEOUT
        )]
    );
}

fn app_server_thread(id: &str, cwd: &str) -> Value {
    json!({
        "id": id,
        "name": "Fictional review",
        "status": {"type": "idle"},
        "cwd": cwd,
        "createdAt": 1790000000_i64,
        "model": "gpt-5",
        "parentThreadId": null,
        "source": "cli",
        "preview": "Review a fictional project"
    })
}

#[test]
fn loaded_threads_follows_cursors_and_skips_a_thread_read_error() {
    let first = app_server_thread("thread-first", "/srv/fictional/first");
    let last = app_server_thread("thread-last", "/srv/fictional/last");
    let expected = vec![first.clone(), last.clone()];
    let (_dir, path, handle) = server(move |mut stream| {
        initialize(&mut stream);
        let mut pages = 0;
        let mut reads = Vec::new();
        let mut ids = std::collections::HashSet::new();
        // Accept either reading each page immediately or collecting all IDs first.
        for _ in 0..5 {
            let call = request(&mut stream);
            let id = call["id"].as_u64().expect("request must have a numeric id");
            assert!(id > 1 && ids.insert(id), "request IDs must be unique");
            match call["method"].as_str().unwrap() {
                "thread/loaded/list" => {
                    let result = match pages {
                        0 => {
                            assert_eq!(call["params"], json!({}));
                            json!({"data":["thread-first","thread-unreadable"],"nextCursor":"fictional-page-two"})
                        }
                        1 => {
                            assert_eq!(call["params"], json!({"cursor":"fictional-page-two"}));
                            json!({"data":["thread-last"],"nextCursor":null})
                        }
                        _ => panic!("unexpected list page"),
                    };
                    pages += 1;
                    send(&mut stream, json!({"id":id,"result":result}));
                }
                "thread/read" => {
                    let thread_id = call["params"]["threadId"].as_str().unwrap();
                    assert_eq!(
                        call["params"],
                        json!({"threadId":thread_id,"includeTurns":false})
                    );
                    assert!(pages >= 1);
                    match thread_id {
                        "thread-first" => {
                            send(&mut stream, json!({"id":id,"result":{"thread":first}}));
                        }
                        "thread-unreadable" => send(
                            &mut stream,
                            json!({"id":id,"error":{"code":-32600,"message":"fictional thread unavailable"}}),
                        ),
                        "thread-last" => {
                            assert_eq!(pages, 2);
                            send(&mut stream, json!({"id":id,"result":{"thread":last}}));
                        }
                        _ => panic!("unexpected thread ID"),
                    }
                    reads.push(thread_id.to_owned());
                }
                _ => panic!("unexpected app-server method"),
            }
        }
        assert_eq!(pages, 2);
        reads.sort();
        assert_eq!(reads, ["thread-first", "thread-last", "thread-unreadable"]);
    });
    let mut rpc = CodexRpc::connect(&path, TIMEOUT).unwrap();
    let threads = rpc.loaded_threads(TIMEOUT).unwrap();
    assert_eq!(threads.len(), expected.len());
    for thread in expected {
        assert!(threads.contains(&thread));
    }
    handle.join().unwrap();
}

#[test]
fn poll_combines_sources_and_keeps_codex_when_claude_fails_or_returns_invalid_json() {
    let dir = TempDir::new();
    let repo = dir.path().join("fictional-repository");
    let cwd = repo.join("nested");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(&cwd).unwrap();
    let home = dir.path().join("claude");
    let transcripts = home.join("projects/fictional-project");
    fs::create_dir_all(&transcripts).unwrap();
    fs::write(
        transcripts.join("transcript-example.jsonl"),
        transcript("claude-opus-5-5"),
    )
    .unwrap();
    let input = agents(cwd.to_str().unwrap());
    let thread = app_server_thread("thread-combined", cwd.to_str().unwrap());
    let parsed = parse_codex_thread(&thread).unwrap();
    let codex = codex_session(
        &parsed,
        project_for(&parsed.cwd, |path| {
            std::path::Path::new(path).join(".git").exists()
        }),
    )
    .unwrap();
    let (_socket_dir, path, handle) = server(move |mut stream| {
        initialize(&mut stream);
        for _ in 0..3 {
            let list = request(&mut stream);
            assert_eq!(list["method"], "thread/loaded/list");
            assert_eq!(list["params"], json!({}));
            send(
                &mut stream,
                json!({"id":list["id"],"result":{"data":["thread-combined"],"nextCursor":null}}),
            );
            let read = request(&mut stream);
            assert_eq!(read["method"], "thread/read");
            assert_eq!(
                read["params"],
                json!({"threadId":"thread-combined","includeTurns":false})
            );
            send(
                &mut stream,
                json!({"id":read["id"],"result":{"thread":thread}}),
            );
        }
    });
    let (mut live, calls) = source(
        Some(home),
        Some(path),
        vec![
            output(true, &input, ""),
            output(false, &input, "fictional Claude failure"),
            output(true, "invalid json", ""),
        ],
    );
    let mut expected = expected_claude(&input, Some("opus"));
    assert!(!expected.is_empty());
    expected.push(codex.clone());
    let sessions = live.poll();
    assert_eq!(sessions.len(), expected.len());
    for session in expected {
        assert!(sessions.contains(&session));
    }
    for _ in 0..2 {
        assert_eq!(live.poll(), vec![codex.clone()]);
    }
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            (
                vec!["claude".into(), "agents".into(), "--json".into()],
                TIMEOUT
            );
            3
        ]
    );
    handle.join().unwrap();
}
