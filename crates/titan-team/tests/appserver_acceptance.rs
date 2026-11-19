//! Acceptance tests for the public app-server CLI contract (C1-C11).
#![cfg(unix)]

mod support;

use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use support::{FakeCodex, methods, prompt, requests, user_turn};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    fake: PathBuf,
}

impl Fixture {
    fn new(project: &str, role: &str, top: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "paper-appserver-acceptance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".agents")).unwrap();
        let fake = root.join("fake");
        fs::create_dir_all(&fake).unwrap();
        let f = Self { root, fake };
        f.git(&["init", "-b", "main"]);
        f.git(&["config", "user.name", "Paper Fixture"]);
        f.git(&["config", "user.email", "paper@example.invalid"]);
        f.git(&["commit", "--allow-empty", "-m", "Initial paper"]);
        fs::write(f.root.join("brief.md"), "Build a paper beacon.").unwrap();
        fs::write(f.root.join("prompt.md"), "Inspect the paper design.").unwrap();
        fs::write(
            f.root.join("schema.json"),
            r#"{"type":"object","properties":{"report":{"type":"string"}}}"#,
        )
        .unwrap();
        fs::write(
            f.root.join(".agents/team.toml"),
            format!(
                r#"
version = 1
name = "paper"
{top}
[project]
autocommit = true
{project}
[roles.lead]
cli = "claude"
model = "lead-model"
launch = false
[roles.writer]
cli = "codex"
model = "paper-model"
effort = "low"
sandbox = "workspace-write"
prompt_file = "prompt.md"
timeout_s = 1
{role}
"#
            ),
        )
        .unwrap();
        FakeCodex::default().install(&f.fake);
        let out = f.run(&["new", "paper", "--brief", "brief.md"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        f
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_titan-team"))
            .current_dir(&self.root)
            .env("TITAN_TEAM_CODEX", self.fake.join("codex"))
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .args(args)
            .output()
            .unwrap()
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn worktree(&self) -> PathBuf {
        self.root.join(".agents/worktrees/paper")
    }
    fn state_path(&self) -> PathBuf {
        self.root.join(".agents/state/paper.json")
    }
    fn saved(&self, name: &str) -> PathBuf {
        self.root.join(".agents/state/paper").join(name)
    }
    fn state(&self) -> Value {
        serde_json::from_slice(&fs::read(self.state_path()).unwrap()).unwrap()
    }
    fn events(&self) -> Vec<Value> {
        read_json_lines(&self.saved("events.jsonl"))
    }
    fn clear_requests(&self) {
        let _ = fs::remove_file(self.fake.join("requests.jsonl"));
    }
    fn first_turn(&self) {
        let out = self.run(&["ask", "writer", "paper"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        self.clear_requests();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn read_json_lines(path: &std::path::Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

fn request(dir: &std::path::Path, method: &str) -> Value {
    requests(dir)
        .into_iter()
        .find(|r| r["method"] == method)
        .unwrap()
}

#[test]
fn new_turn_rpc_contract() {
    let f = Fixture::new(
        "target_dir = \"paper-target\"",
        r#"output_schema = "schema.json"
config = ['model_verbosity="low"', 'tools.web_search=false', 'paper.count=5', 'paper.flag=true', 'paper.names=["a"]', 'paper.raw=not valid toml']"#,
        r#"[codex]
config = ['model_verbosity="high"']"#,
    );
    let fake = FakeCodex {
        answer: r#"{"report":"Paper complete"}"#.into(),
        usage: (13, 4, 7),
        activity: vec![
            json!({"type":"commandExecution","command":"/usr/bin/bash -lc 'cargo test'"}),
            json!({"type":"fileChange","changes":[{"path":f.worktree().join("kite.txt")},{"path":"sail.txt"}]}),
            json!({"type":"mcpToolCall"}),
            json!({"type":"webSearch"}),
            json!({"type":"unknownPaperItem"}),
        ],
        ..FakeCodex::default()
    };
    fake.install(&f.fake);
    let out = f.run(&[
        "ask",
        "writer",
        "paper",
        "-m",
        "Check \"folds\".\nThen report.",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        methods(&f.fake),
        [
            "initialize",
            "initialized",
            "thread/start",
            "turn/start",
            "thread/unsubscribe"
        ]
    );
    let raw = fs::read_to_string(f.fake.join("requests.jsonl")).unwrap();
    let mut next = 1_u64;
    for line in raw.lines() {
        let value: Value = serde_json::from_str(line).unwrap();
        if value.get("id").is_some() {
            assert_eq!(value["id"], next);
            assert!(line.starts_with(&format!(
                "{{\"id\":{next},\"method\":\"{}\",\"params\":{{",
                value["method"].as_str().unwrap()
            )));
            // Whitespace inside string values is allowed; structural whitespace is not.
            assert_eq!(serde_json::to_string(&value).unwrap().len(), line.len());
            next += 1;
        } else {
            assert_eq!(line, r#"{"method":"initialized"}"#);
        }
    }
    let init = request(&f.fake, "initialize");
    assert_eq!(
        init["params"],
        json!({"clientInfo":{"name":"titan-team","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})
    );
    let start = request(&f.fake, "thread/start");
    assert_eq!(start["params"]["cwd"], f.worktree().to_str().unwrap());
    assert_eq!(start["params"]["model"], "paper-model");
    assert_eq!(start["params"]["sandbox"], "workspace-write");
    assert_eq!(start["params"]["approvalPolicy"], "never");
    let target = f.root.join("paper-target");
    assert_eq!(
        start["params"]["config"],
        json!({
            "model_reasoning_effort":"low", "model_verbosity":"low", "tools.web_search":false,
            "paper.count":5,"paper.flag":true,"paper.names":["a"],"paper.raw":"not valid toml",
            "sandbox_workspace_write.writable_roots":[target],
            "shell_environment_policy.set":{"CARGO_TARGET_DIR":target}
        })
    );
    assert_eq!(
        fs::read_to_string(f.fake.join("codex-args")).unwrap(),
        "app-server\n"
    );
    assert_eq!(
        fs::read_to_string(f.fake.join("codex-cwd")).unwrap().trim(),
        f.worktree().to_str().unwrap()
    );
    assert_eq!(
        fs::read_to_string(f.fake.join("codex-target")).unwrap(),
        target.to_str().unwrap()
    );
    assert_eq!(
        prompt(&f.fake),
        "Inspect the paper design.\n\n# Encargo\n\nBuild a paper beacon.\n\n# Mensaje del director\n\nCheck \"folds\".\nThen report."
    );
    let turn = request(&f.fake, "turn/start");
    assert_eq!(turn["params"]["threadId"], "paper-thread");
    assert_eq!(turn["params"]["input"][0]["type"], "text");
    assert_eq!(
        turn["params"]["outputSchema"],
        serde_json::from_str::<Value>(&fs::read_to_string(f.root.join("schema.json")).unwrap())
            .unwrap()
    );
    assert_eq!(
        fs::read_to_string(f.saved("01-writer.json")).unwrap(),
        fake.answer
    );
    let events = f.events();
    let activity: Vec<_> = events
        .iter()
        .filter(|e| e["type"] == "activity")
        .map(|e| e["text"].as_str().unwrap())
        .collect();
    assert_eq!(
        activity,
        [
            "$ cargo test",
            "edita kite.txt, sail.txt",
            "mcp_tool_call",
            "web_search"
        ]
    );
    let history = &f.state()["history"][0];
    assert_eq!(history["thread_id"], "paper-thread");
    assert_eq!(history["turn_id"], "paper-turn");
    assert_eq!(
        history["usage"],
        json!({"input_tokens":13,"cached_input_tokens":4,"output_tokens":7})
    );
    let log = read_json_lines(&f.saved("01-writer.jsonl"));
    assert_eq!(log.len(), 9);
    assert!(
        log.iter()
            .all(|m| m.get("method").is_some() && m.get("id").is_none())
    );
    assert_eq!(log.first().unwrap()["method"], "turn/started");
    assert_eq!(log.last().unwrap()["method"], "turn/completed");
    assert_eq!(fs::read_to_string(f.saved("01-writer.stderr")).unwrap(), "");
    let answer: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(answer["user_turns"], Value::Null);
}

#[test]
fn connection_fallback_and_missing_executable() {
    for version in [
        "not JSON",
        r#"{"status":"stopped"}"#,
        r#"{"status":"running","socketPath":"/tmp/paper-absent-socket/no.sock"}"#,
    ] {
        let f = Fixture::new("", "", "");
        FakeCodex {
            daemon_version: version.into(),
            ..FakeCodex::default()
        }
        .install(&f.fake);
        let out = f.run(&["ask", "writer", "paper"]);
        assert_eq!(out.status.code(), Some(0), "{version}: {out:?}");
        assert_eq!(
            fs::read_to_string(f.fake.join("codex-args")).unwrap(),
            "app-server\n"
        );
    }
    let f = Fixture::new("", "", "");
    fs::remove_file(f.fake.join("codex")).unwrap();
    let before = fs::read(f.saved("events.jsonl")).unwrap();
    let out = f.run(&["ask", "writer", "paper"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("no está disponible"));
    assert_eq!(fs::read(f.saved("events.jsonl")).unwrap(), before);
}

#[test]
fn resume_accounts_for_user_turns() {
    for json_output in [false, true] {
        let f = Fixture::new("", "", "");
        f.first_turn();
        let old_text = format!("{}end", "p".repeat(210));
        FakeCodex {
            turn_id: "paper-next".into(),
            turns: json!([
                user_turn("user-new", "Newest"),
                user_turn("user-old", &old_text),
                user_turn("paper-turn", "Known"),
                user_turn("before-known", "Ignore")
            ]),
            ..FakeCodex::default()
        }
        .install(&f.fake);
        fs::write(f.worktree().join("user-note.txt"), "A user fold.\n").unwrap();
        let args = if json_output {
            vec!["ask", "writer", "paper", "-m", "Continue.", "--json"]
        } else {
            vec!["ask", "writer", "paper", "-m", "Continue."]
        };
        let out = f.run(&args);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_eq!(
            methods(&f.fake),
            [
                "initialize",
                "initialized",
                "thread/read",
                "thread/turns/list",
                "thread/resume",
                "turn/start",
                "thread/unsubscribe"
            ]
        );
        assert_eq!(
            request(&f.fake, "thread/read")["params"],
            json!({"threadId":"paper-thread"})
        );
        assert_eq!(
            request(&f.fake, "thread/turns/list")["params"],
            json!({"threadId":"paper-thread","limit":50})
        );
        let resume = request(&f.fake, "thread/resume");
        assert!(resume["params"].get("cwd").is_none());
        assert_eq!(resume["params"]["threadId"], "paper-thread");
        assert_eq!(resume["params"]["model"], "paper-model");
        assert_eq!(resume["params"]["sandbox"], "workspace-write");
        assert_eq!(resume["params"]["approvalPolicy"], "never");
        assert_eq!(
            resume["params"]["config"],
            json!({"model_reasoning_effort":"low"})
        );
        let events = f.events();
        let index = events
            .iter()
            .position(|e| e["type"] == "user_turn")
            .unwrap();
        let event = &events[index];
        assert_eq!(event["v"], 1);
        assert_eq!(event["role"], "writer");
        assert_eq!(event["thread_id"], "paper-thread");
        assert_eq!(event["turns"], 2);
        // Read "truncated to 200 characters with an ellipsis" as a 200-character total.
        assert_eq!(event["text"], format!("{}…", "p".repeat(199)));
        assert_eq!(event["changed_files"], json!(["user-note.txt"]));
        let commit = event["commit"].as_str().unwrap();
        assert!(!commit.is_empty());
        assert_eq!(
            f.git(&[
                "-C",
                f.worktree().to_str().unwrap(),
                "show",
                "-s",
                "--format=%s",
                commit
            ]),
            "team: turnos del usuario con writer"
        );
        assert_eq!(
            f.git(&[
                "-C",
                f.worktree().to_str().unwrap(),
                "show",
                &format!("{commit}:user-note.txt")
            ]),
            "A user fold."
        );
        assert!(
            events
                .iter()
                .enumerate()
                .filter(|(_, e)| e["type"] == "brief_sent")
                .last()
                .unwrap()
                .0
                > index
        );
        assert_eq!(f.state()["history"][1]["turn_id"], "paper-next");
        if json_output {
            let answer: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(
                answer["user_turns"],
                json!({"turns":2,"text":event["text"],"commit":commit})
            );
        } else {
            let stdout = String::from_utf8(out.stdout).unwrap();
            let line = stdout
                .lines()
                .find(|l| l.starts_with("Turnos del usuario con writer: 2"))
                .unwrap();
            assert!(stdout.find(line).unwrap() < stdout.find("Paper response").unwrap());
        }
    }
    // Legacy entries without turn_id must not classify every returned turn as a user turn.
    let f = Fixture::new("", "", "");
    f.first_turn();
    let mut state = f.state();
    state["history"][0]
        .as_object_mut()
        .unwrap()
        .remove("turn_id");
    fs::write(f.state_path(), serde_json::to_vec(&state).unwrap()).unwrap();
    FakeCodex {
        turns: json!([user_turn("unmatched", "Old session")]),
        ..FakeCodex::default()
    }
    .install(&f.fake);
    let out = f.run(&["ask", "writer", "paper", "-m", "Continue.", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["user_turns"],
        Value::Null
    );
    assert!(!f.events().iter().any(|e| e["type"] == "user_turn"));
}

#[test]
fn busy_preserves_role_and_worktree() {
    for active in [true, false] {
        let f = Fixture::new("", "", "");
        f.first_turn();
        let before = f.state()["roles"]["writer"].clone();
        let head = f.git(&["-C", f.worktree().to_str().unwrap(), "rev-parse", "HEAD"]);
        fs::write(f.worktree().join("pending.txt"), "Keep pending").unwrap();
        FakeCodex {
            thread_status: if active { "active" } else { "idle" }.into(),
            resume_error: if active {
                None
            } else {
                Some("Thread ALREADY HAS AN ACTIVE WRITER".into())
            },
            ..FakeCodex::default()
        }
        .install(&f.fake);
        let out = f.run(&["ask", "writer", "paper", "-m", "Try again."]);
        assert_eq!(out.status.code(), Some(4), "{out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .any(|l| l.starts_with("El hilo de writer está ocupado"))
        );
        let calls = methods(&f.fake);
        assert!(
            !calls
                .iter()
                .any(|m| m == "turn/start" || m == "thread/start")
        );
        // C5's active-thread branch explicitly says that nothing more is sent.
        if active {
            assert_eq!(calls, ["initialize", "initialized", "thread/read"]);
        }
        assert_eq!(
            f.state()["roles"]["writer"]["thread_id"],
            before["thread_id"]
        );
        assert_eq!(f.state()["roles"]["writer"]["turns"], before["turns"]);
        assert_eq!(
            f.git(&["-C", f.worktree().to_str().unwrap(), "rev-parse", "HEAD"]),
            head
        );
        let state = f.state();
        let history = state["history"].as_array().unwrap().last().unwrap();
        assert_eq!(history["turn_id"], Value::Null);
        let events = f.events();
        let finish = events
            .iter()
            .rev()
            .find(|e| e["type"] == "turn_finished")
            .unwrap();
        assert_eq!(finish["status"], "busy");
        assert_eq!(finish["text"], "hilo de writer ocupado");
    }
}

#[test]
fn turn_errors_and_cleanup() {
    for case in ["rpc", "failed", "closed"] {
        let f = Fixture::new("", "", "");
        if case == "rpc" {
            f.first_turn();
        }
        FakeCodex {
            resume_error: if case == "rpc" {
                Some("Paper resume rejected".into())
            } else {
                None
            },
            status: if case == "failed" {
                "failed"
            } else {
                "completed"
            }
            .into(),
            exit: if case == "closed" { Some(7) } else { None },
            ..FakeCodex::default()
        }
        .install(&f.fake);
        let out = f.run(&["ask", "writer", "paper", "-m", "Inspect."]);
        assert_eq!(out.status.code(), Some(1), "{case}: {out:?}");
        let events = f.events();
        assert_eq!(
            events
                .iter()
                .rev()
                .find(|e| e["type"] == "turn_finished")
                .unwrap()["status"],
            "error"
        );
        let name = if case == "rpc" {
            "02-writer.stderr"
        } else {
            "01-writer.stderr"
        };
        let stderr = fs::read_to_string(f.saved(name)).unwrap();
        if case == "rpc" {
            assert!(stderr.contains("Paper resume rejected"));
        }
        if case == "failed" {
            assert!(stderr.contains("turn failed"));
        }
        // A server that already exited cannot receive an unsubscribe.
        if case != "closed" {
            assert_eq!(methods(&f.fake).last().unwrap(), "thread/unsubscribe");
        }
    }
}

#[test]
fn timeout_interrupts_then_unsubscribes_and_keeps_timeout_status() {
    let f = Fixture::new("", "", "");
    FakeCodex {
        hang: true,
        ..FakeCodex::default()
    }
    .install(&f.fake);
    let started = std::time::Instant::now();
    let out = f.run(&["ask", "writer", "paper"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
    let calls = methods(&f.fake);
    assert_eq!(
        &calls[calls.len() - 2..],
        ["turn/interrupt", "thread/unsubscribe"]
    );
    assert_eq!(
        request(&f.fake, "turn/interrupt")["params"],
        json!({"threadId":"paper-thread","turnId":"paper-turn"})
    );
    assert_eq!(
        request(&f.fake, "thread/unsubscribe")["params"],
        json!({"threadId":"paper-thread"})
    );
    assert_eq!(f.state()["history"][0]["status"], "timeout");
    assert_eq!(f.state()["history"][0]["turn_id"], "paper-turn");
    let log = read_json_lines(&f.saved("01-writer.jsonl"));
    assert_eq!(
        log.last().unwrap()["params"]["turn"]["status"],
        "interrupted"
    );
}

#[test]
fn dry_run_has_no_side_effects() {
    let f = Fixture::new("", "", "");
    for resumed in [false, true] {
        if resumed {
            f.first_turn();
        }
        for force_new in [false, true] {
            // Removing the executable makes any accidental probe observable as failure.
            let _ = fs::remove_file(f.fake.join("codex"));
            let before_state = fs::read(f.state_path()).unwrap();
            let before_events = fs::read(f.saved("events.jsonl")).unwrap();
            let mut args = vec![
                "ask",
                "writer",
                "paper",
                "-m",
                "Fold\ncarefully.",
                "--dry-run",
            ];
            if force_new {
                args.push("--new");
            }
            let out = f.run(&args);
            assert_eq!(out.status.code(), Some(0), "{out:?}");
            let text = String::from_utf8(out.stdout).unwrap();
            let (_, rest) = text.split_once("Peticiones JSON-RPC:\n").unwrap();
            let (rpc, printed_prompt) = rest.split_once("--- prompt ---\n").unwrap();
            let lines: Vec<_> = rpc.lines().filter(|l| !l.is_empty()).collect();
            assert_eq!(lines.len(), 3);
            let thread_method = if resumed && !force_new {
                "thread/resume"
            } else {
                "thread/start"
            };
            for (i, method) in ["initialize", thread_method, "turn/start"]
                .iter()
                .enumerate()
            {
                assert!(lines[i].starts_with(&format!(
                    "{{\"id\":{},\"method\":\"{method}\",\"params\":{{",
                    i + 1
                )));
            }
            let turn: Value = serde_json::from_str(lines[2]).unwrap();
            assert_eq!(
                turn["params"]["input"][0]["text"].as_str().unwrap(),
                printed_prompt.trim_end_matches('\n')
            );
            assert!(printed_prompt.contains("# Mensaje del director\n\nFold\ncarefully."));
            assert!(!f.fake.join("requests.jsonl").exists());
            assert_eq!(fs::read(f.state_path()).unwrap(), before_state);
            assert_eq!(fs::read(f.saved("events.jsonl")).unwrap(), before_events);
            FakeCodex::default().install(&f.fake);
        }
    }
}

#[test]
fn doc_describes_appserver() {
    let out = Command::new(env!("CARGO_BIN_EXE_titan-team"))
        .arg("doc")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    for term in ["app-server", "user_turn", "turn_id"] {
        assert!(text.contains(term), "missing {term}");
    }
    // The documented transport must no longer instruct executing a turn with this command.
    assert!(!text.contains("se ejecuta con `codex exec`"));
    assert!(!text.contains("se ejecuta con codex exec"));
}
