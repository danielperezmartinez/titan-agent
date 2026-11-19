//! Own tests of titan-team's turns through the Codex app-server (ADR-0013);
//! the acceptance tests are in `appserver_acceptance.rs`. This file also
//! checks the fake Codex of `support/` itself.
#![cfg(unix)]

mod support;

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};
use support::{FakeCodex, methods, requests};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "titan-appserver-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Feeds `lines` to the fake's stdio server and returns what it printed.
fn talk(fake: &FakeCodex, lines: &[Value]) -> (PathBuf, Vec<Value>) {
    let dir = temp_dir("fake");
    let codex = fake.install(&dir);
    // Another test thread may fork while the script is still open for
    // writing ("Text file busy"): try again a few times.
    let mut tries = 0;
    let mut child = loop {
        match Command::new(&codex)
            .arg("app-server")
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(child) => break child,
            Err(error) if error.raw_os_error() == Some(26) && tries < 20 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(error) => panic!("{error}"),
        }
    };
    let mut stdin = child.stdin.take().unwrap();
    for line in lines {
        writeln!(stdin, "{line}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let answers = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (dir, answers)
}

#[test]
fn the_fake_codex_answers_a_whole_turn() {
    let fake = FakeCodex {
        activity: vec![json!({"type": "commandExecution", "command": "ls"})],
        body: "printf 'it is a kite' > kite.txt".into(),
        answer: "It's \"done\"\nreally".into(),
        ..FakeCodex::default()
    };
    let (dir, answers) = talk(
        &fake,
        &[
            json!({"id": 1, "method": "initialize", "params": {}}),
            json!({"method": "initialized"}),
            json!({"id": 2, "method": "thread/start", "params": {}}),
            json!({"id": 3, "method": "turn/start", "params": {"input": [{"type": "text", "text": "line one\nit's two"}]}}),
            json!({"id": 4, "method": "thread/unsubscribe", "params": {}}),
        ],
    );
    let kinds: Vec<String> = answers
        .iter()
        .map(|a| {
            a["method"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("#{}", a["id"]))
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "#1",
            "#2",
            "#3",
            "turn/started",
            "item/started",
            "item/completed",
            "thread/tokenUsage/updated",
            "turn/completed",
            "#4"
        ]
    );
    assert_eq!(answers[1]["result"]["thread"]["id"], "paper-thread");
    assert_eq!(
        answers[5]["params"]["item"]["text"],
        "It's \"done\"\nreally"
    );
    assert_eq!(answers[7]["params"]["turn"]["status"], "completed");
    assert_eq!(
        fs::read_to_string(dir.join("kite.txt")).unwrap(),
        "it is a kite"
    );
    assert!(dir.join("codex-started").exists());
    assert_eq!(support::prompt(&dir), "line one\nit's two");
    assert_eq!(methods(&dir).len(), 5);
}

#[test]
fn the_fake_codex_refuses_resume_and_hangs_until_interrupted() {
    let fake = FakeCodex {
        resume_error: Some("thread t already has an active writer".into()),
        hang: true,
        turns: json!([support::user_turn("u1", "hi")]),
        ..FakeCodex::default()
    };
    let (dir, answers) = talk(
        &fake,
        &[
            json!({"id": 1, "method": "thread/resume", "params": {}}),
            json!({"id": 2, "method": "thread/turns/list", "params": {}}),
            json!({"id": 3, "method": "turn/start", "params": {"input": []}}),
            json!({"id": 4, "method": "turn/interrupt", "params": {}}),
        ],
    );
    assert_eq!(
        answers[0]["error"]["message"],
        "thread t already has an active writer"
    );
    assert_eq!(answers[1]["result"]["data"][0]["id"], "u1");
    let last = answers.last().unwrap();
    assert_eq!(last["params"]["turn"]["status"], "interrupted");
    assert!(
        !answers[..answers.len() - 2]
            .iter()
            .any(|a| a["method"] == "turn/completed")
    );
    assert_eq!(requests(&dir).len(), 4);
}

/// A shared daemon: `daemon version` says it runs on a unix socket, where a
/// WebSocket server answers one turn. The own stdio server must not start.
#[test]
fn a_running_daemon_is_used_through_its_socket() {
    use std::os::unix::net::UnixListener;
    use tungstenite::{Message, accept};

    let root = temp_dir("daemon");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.name", "Paper Fixture"]);
    git(&["config", "user.email", "paper@example.invalid"]);
    git(&["commit", "-q", "--allow-empty", "-m", "start"]);
    fs::create_dir_all(root.join(".agents")).unwrap();
    fs::write(root.join("prompt.md"), "Fold the kite.").unwrap();
    fs::write(root.join("brief.md"), "Fold the kite.").unwrap();
    fs::write(
        root.join(".agents/team.toml"),
        "version = 1\nname = \"paper\"\n[project]\nautocommit = true\n\
         [roles.lead]\ncli = \"claude\"\nmodel = \"lead\"\nlaunch = false\n\
         [roles.writer]\ncli = \"codex\"\nmodel = \"paper-model\"\nsandbox = \"read-only\"\n\
         prompt_file = \"prompt.md\"\ntimeout_s = 10\n",
    )
    .unwrap();
    let socket = root.join("daemon.sock");
    let fake = FakeCodex {
        daemon_version: json!({"status": "running", "socketPath": socket}).to_string(),
        ..FakeCodex::default()
    };
    let codex = fake.install(&root);
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut ws = accept(stream).unwrap();
        let mut seen = Vec::new();
        let send = |ws: &mut tungstenite::WebSocket<_>, value: Value| {
            ws.send(Message::text(value.to_string())).unwrap();
        };
        loop {
            let message = match ws.read() {
                Ok(Message::Text(text)) => text.to_string(),
                Ok(_) => continue,
                Err(_) => break,
            };
            let request: Value = serde_json::from_str(&message).unwrap();
            let method = request["method"].as_str().unwrap_or_default().to_string();
            seen.push(method.clone());
            let Some(id) = request.get("id").cloned() else {
                continue;
            };
            let result = match method.as_str() {
                "thread/start" => json!({"thread": {"id": "sock-thread"}}),
                "turn/start" => json!({"turn": {"id": "sock-turn"}}),
                _ => json!({}),
            };
            send(&mut ws, json!({"id": id, "result": result}));
            if method == "turn/start" {
                let p = |m: &str, params: Value| json!({"method": m, "params": params});
                // Another thread's notification must be ignored.
                send(
                    &mut ws,
                    p(
                        "item/completed",
                        json!({"threadId": "other", "turnId": "x", "item": {"type": "agentMessage", "text": "wrong"}}),
                    ),
                );
                send(
                    &mut ws,
                    p(
                        "item/completed",
                        json!({"threadId": "sock-thread", "turnId": "sock-turn", "item": {"type": "agentMessage", "text": "From the daemon"}}),
                    ),
                );
                for _ in 0..2 {
                    send(
                        &mut ws,
                        p(
                            "thread/tokenUsage/updated",
                            json!({"threadId": "sock-thread", "turnId": "sock-turn", "tokenUsage": {"last": {"inputTokens": 5, "cachedInputTokens": 2, "outputTokens": 1}}}),
                        ),
                    );
                }
                send(
                    &mut ws,
                    p(
                        "turn/completed",
                        json!({"threadId": "sock-thread", "turn": {"id": "sock-turn", "status": "completed"}}),
                    ),
                );
            }
            if method == "thread/unsubscribe" {
                break;
            }
        }
        seen
    });
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_titan-team"))
            .current_dir(&root)
            .env("TITAN_TEAM_CODEX", &codex)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .args(args)
            .output()
            .unwrap()
    };
    assert_eq!(
        run(&["new", "kite", "--brief", "brief.md"]).status.code(),
        Some(0)
    );
    let out = run(&["ask", "writer", "kite", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let seen = server.join().unwrap();
    assert_eq!(
        seen,
        [
            "initialize",
            "initialized",
            "thread/start",
            "turn/start",
            "thread/unsubscribe"
        ]
    );
    assert!(
        !root.join("requests.jsonl").exists(),
        "the stdio server must not start"
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["text"], "From the daemon");
    assert_eq!(report["thread_id"], "sock-thread");
    assert_eq!(report["turn_id"], "sock-turn");
    assert_eq!(
        report["usage"]["input_tokens"], 10,
        "usage adds every `last`"
    );
    assert_eq!(report["usage"]["cached_input_tokens"], 4);
    assert_eq!(report["user_turns"], Value::Null);
}

/// A repo with a writing role `writer` and the task `kite`, whose fake
/// Codex lives in the repo root.
fn kite_repo() -> PathBuf {
    kite_repo_with("")
}

/// Like [`kite_repo`], with extra `[project]` lines.
fn kite_repo_with(project: &str) -> PathBuf {
    let root = temp_dir("kite");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.name", "Paper Fixture"]);
    git(&["config", "user.email", "paper@example.invalid"]);
    git(&["commit", "-q", "--allow-empty", "-m", "start"]);
    fs::create_dir_all(root.join(".agents")).unwrap();
    fs::write(root.join("prompt.md"), "Fold the kite.").unwrap();
    fs::write(root.join("brief.md"), "Fold the kite.").unwrap();
    fs::write(
        root.join(".agents/team.toml"),
        format!(
            "version = 1\nname = \"paper\"\n[project]\nautocommit = true\n{project}\n\
             [roles.lead]\ncli = \"claude\"\nmodel = \"lead\"\nlaunch = false\n\
             [roles.writer]\ncli = \"codex\"\nmodel = \"paper-model\"\nsandbox = \"workspace-write\"\n\
             prompt_file = \"prompt.md\"\ntimeout_s = 2\n"
        ),
    )
    .unwrap();
    FakeCodex::default().install(&root);
    assert_eq!(
        titan(&root, &["new", "kite", "--brief", "brief.md"])
            .status
            .code(),
        Some(0)
    );
    root
}

fn titan(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_titan-team"))
        .current_dir(root)
        .env("TITAN_TEAM_CODEX", root.join("codex"))
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn a_busy_resume_records_no_user_turns_so_they_are_told_once() {
    let root = kite_repo();
    assert_eq!(
        titan(&root, &["ask", "writer", "kite"]).status.code(),
        Some(0)
    );
    let turns = json!([support::user_turn("u1", "fold it twice"), {"id": "paper-turn", "status": "completed", "items": []}]);
    FakeCodex {
        turns: turns.clone(),
        resume_error: Some("thread paper-thread already has an active writer".into()),
        ..FakeCodex::default()
    }
    .install(&root);
    let busy = titan(&root, &["ask", "writer", "kite", "-m", "again"]);
    assert_eq!(busy.status.code(), Some(4), "{busy:?}");
    let events = || fs::read_to_string(root.join(".agents/state/kite/events.jsonl")).unwrap();
    assert!(
        !events().contains("\"user_turn\""),
        "busy: nothing about the user yet"
    );
    FakeCodex {
        turns,
        turn_id: "paper-turn-2".into(),
        ..FakeCodex::default()
    }
    .install(&root);
    let out = titan(&root, &["ask", "writer", "kite", "-m", "again"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("Turnos del usuario con writer: 1"));
    assert_eq!(events().matches("\"user_turn\"").count(), 1);
}

/// Writes `root/codex`: a hand-made app-server whose loop body is `on_line`
/// (sh, with `$line` and `$id` set); `daemon version` says it is stopped.
fn script_codex(root: &std::path::Path, on_line: &str) {
    use std::os::unix::fs::PermissionsExt;
    let script = format!(
        r#"#!/bin/sh
if [ "$2" = daemon ]; then echo '{{"status":"stopped"}}'; exit 0; fi
echo $$ > "$(dirname "$0")/server.pid"
while IFS= read -r line; do
  id=$(printf '%s\n' "$line" | sed -n 's/^{{"id":\([0-9][0-9]*\),.*/\1/p')
  [ -n "$id" ] || continue
{on_line}
done
"#
    );
    let path = root.join("codex");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

const ANSWER_SETUP: &str = r#"  case "$line" in
  *'"method":"initialize"'*) printf '{"id":%s,"result":{}}\n' "$id" ;;
  *'"method":"thread/start"'*) printf '{"id":%s,"result":{"thread":{"id":"t"}}}\n' "$id" ;;
  esac"#;

fn last_finished(root: &std::path::Path, task: &str) -> Value {
    fs::read_to_string(root.join(format!(".agents/state/{task}/events.jsonl")))
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .rfind(|e| e["type"] == "turn_finished")
        .unwrap()
}

#[test]
fn a_server_that_never_stops_writing_still_times_out_and_is_not_waited_twice() {
    let root = kite_repo();
    // After turn/start it floods notifications and never answers again,
    // not even turn/interrupt.
    script_codex(
        &root,
        &format!(
            "{ANSWER_SETUP}\n  case \"$line\" in *'\"method\":\"turn/start\"'*)\n    printf '{{\"id\":%s,\"result\":{{\"turn\":{{\"id\":\"u\"}}}}}}\\n' \"$id\"\n    while :; do printf '%s\\n' '{{\"method\":\"noise\",\"params\":{{\"threadId\":\"t\",\"turnId\":\"u\"}}}}'; done ;;\n  esac"
        ),
    );
    let started = std::time::Instant::now();
    let out = titan(&root, &["ask", "writer", "kite"]);
    let took = started.elapsed();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(last_finished(&root, "kite")["status"], "timeout");
    // 2 s of turn, at most 10 s for the interrupt, 3 s for the unsubscribe
    // and 3 s to end the server.
    assert!(took < std::time::Duration::from_secs(20), "took {took:?}");
}

#[test]
fn a_failed_initialize_ends_its_server() {
    let root = kite_repo();
    script_codex(
        &root,
        r#"  printf '{"id":%s,"error":{"code":1,"message":"no init"}}\n' "$id"
  trap '' TERM HUP
  exec sleep 60 < /dev/null"#,
    );
    let out = titan(&root, &["ask", "writer", "kite"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = fs::read_to_string(root.join(".agents/state/kite/01-writer.stderr")).unwrap();
    assert!(stderr.contains("no init"), "{stderr}");
    let pid = fs::read_to_string(root.join("server.pid")).unwrap();
    let alive = Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .unwrap();
    assert!(!alive.success(), "the server must not outlive ask");
}

#[test]
fn a_server_that_exits_mid_turn_is_an_error_even_if_a_child_holds_its_output() {
    let root = kite_repo();
    script_codex(
        &root,
        &format!(
            "{ANSWER_SETUP}\n  case \"$line\" in *'\"method\":\"turn/start\"'*)\n    printf '{{\"id\":%s,\"result\":{{\"turn\":{{\"id\":\"u\"}}}}}}\\n' \"$id\"\n    sleep 30 &\n    exit 0 ;;\n  esac"
        ),
    );
    let started = std::time::Instant::now();
    let out = titan(&root, &["ask", "writer", "kite"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(last_finished(&root, "kite")["status"], "error");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2) + std::time::Duration::from_secs(5)
    );
}

#[test]
fn a_codex_that_cannot_run_writes_nothing_and_exits_2() {
    use std::os::unix::fs::PermissionsExt;
    let root = kite_repo();
    fs::write(root.join("codex"), "not a program").unwrap();
    fs::set_permissions(root.join("codex"), fs::Permissions::from_mode(0o644)).unwrap();
    let out = titan(&root, &["ask", "writer", "kite"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("no está disponible"));
    let events = fs::read_to_string(root.join(".agents/state/kite/events.jsonl")).unwrap();
    assert!(!events.contains("brief_sent"), "{events}");
}

#[test]
fn a_busy_thread_runs_no_gate() {
    let root = kite_repo_with("gate_cmd = \"touch gate-ran\"");
    assert_eq!(
        titan(&root, &["ask", "writer", "kite"]).status.code(),
        Some(0)
    );
    FakeCodex {
        thread_status: "active".into(),
        ..FakeCodex::default()
    }
    .install(&root);
    let out = titan(&root, &["ask", "writer", "kite", "-m", "again", "--gate"]);
    assert_eq!(out.status.code(), Some(4), "{out:?}");
    assert!(!root.join(".agents/worktrees/kite/gate-ran").exists());
}

#[test]
fn a_failed_commit_of_the_user_turns_stops_the_turn() {
    use std::os::unix::fs::PermissionsExt;
    let root = kite_repo();
    assert_eq!(
        titan(&root, &["ask", "writer", "kite"]).status.code(),
        Some(0)
    );
    FakeCodex {
        turns: json!([support::user_turn("u1", "fold"), {"id": "paper-turn", "items": []}]),
        turn_id: "paper-turn-2".into(),
        ..FakeCodex::default()
    }
    .install(&root);
    fs::write(root.join(".agents/worktrees/kite/user.txt"), "by the user").unwrap();
    let hook = root.join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let out = titan(&root, &["ask", "writer", "kite", "-m", "again"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let starts = support::methods(&root)
        .iter()
        .filter(|m| *m == "turn/start")
        .count();
    assert_eq!(starts, 1, "only the first ask started a turn");
    let events = fs::read_to_string(root.join(".agents/state/kite/events.jsonl")).unwrap();
    assert_eq!(events.matches("brief_sent").count(), 1, "no second turn");
}
