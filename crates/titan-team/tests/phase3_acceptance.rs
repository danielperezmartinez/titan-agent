//! Public CLI acceptance contract. Requires serde_json as a dev dependency.
#![cfg(unix)]

use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

mod support;
use support::{FakeCodex, methods, prompt, requests};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "titan-team-acceptance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".agents")).unwrap();
        let f = Self(root);
        f.git(&["init", "-b", "main"]);
        f.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "fixture",
        ]);
        fs::write(f.0.join("brief.md"), "Build a paper lighthouse.").unwrap();
        fs::write(f.0.join("prompt.md"), "Inspect the paper design.").unwrap();
        fs::write(
            f.0.join(".agents/team.toml"),
            r#"
version = 1
name = "paper-team"
[project]
repo = "."
base = "main"
[flow]
review_role = "reviewer"
max_review_rounds = 1
[roles.director]
cli = "claude"
model = "fixture-director"
launch = false
[roles.reviewer]
cli = "codex"
model = "fixture-model"
effort = "high"
sandbox = "read-only"
prompt_file = "prompt.md"
"#,
        )
        .unwrap();
        FakeCodex {
            thread_id: "fixture-thread".into(),
            answer: "Final answer\nSecond line".into(),
            activity: vec![json!({"type":"commandExecution", "command":"printf inspect"})],
            usage: (11, 3, 7),
            ..FakeCodex::default()
        }
        .install(&f.0);
        f
    }

    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_titan-team"))
            .current_dir(&self.0)
            .env("TITAN_TEAM_CODEX", self.0.join("codex"))
            .args(args)
            .output()
            .unwrap()
    }

    fn create_task(&self) {
        let output = self.run(&["new", "paper", "--brief", "brief.md"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }

    fn event_bytes(&self) -> Vec<u8> {
        fs::read(self.0.join(".agents/state/paper/events.jsonl")).unwrap()
    }

    fn events(&self) -> Vec<Value> {
        String::from_utf8(self.event_bytes())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn validation_collects_independent_errors_and_blocks_other_commands() {
    let f = Fixture::new();
    fs::write(
        f.0.join(".agents/team.toml"),
        r#"
version = 7
name = "paper-team"
unexpected = true
[project]
test_timeout_s = 0
unexpected = true
[flow]
review_role = "absent"
[roles.worker]
cli = "codex"
sandbox = "danger-full-access"
prompt_file = "missing-prompt.md"
[roles.director]
cli = "claude"
model = "fixture-director"
launch = true
"#,
    )
    .unwrap();
    let output = f.run(&["check", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["ok"], false);
    let errors = result["errors"].as_array().unwrap();
    for (path, code) in [
        ("version", "bad_version"),
        ("unexpected", "unknown_key"),
        ("project.test_timeout_s", "bad_type"),
        ("project.unexpected", "unknown_key"),
        ("flow.review_role", "unknown_role"),
        ("roles.worker.model", "missing_key"),
        ("roles.worker.sandbox", "unknown_value"),
        ("roles.worker.prompt_file", "missing_file"),
        ("roles.director.launch", "unknown_value"),
    ] {
        assert!(
            errors.iter().any(|e| e["path"] == path
                && e["code"] == code
                && e["message"].as_str().is_some_and(|s| !s.is_empty())),
            "missing {path}/{code}: {result}"
        );
    }
    let blocked = f.run(&["new", "paper", "--brief", "brief.md"]);
    assert_eq!(blocked.status.code(), Some(2));
    assert!(!blocked.stderr.is_empty());
    assert!(!f.0.join(".agents/state/paper/events.jsonl").exists());

    fs::write(f.0.join(".agents/team.toml"), "version = [").unwrap();
    let malformed = f.run(&["check", "--json"]);
    assert_eq!(malformed.status.code(), Some(2));
    let result: Value = serde_json::from_slice(&malformed.stdout).unwrap();
    assert_eq!(result["ok"], false);
    assert!(
        result["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["path"] == "" && e["code"] == "parse_error")
    );
}

#[test]
fn a_codex_turn_uses_stdin_and_appends_versioned_events_in_order() {
    let f = Fixture::new();
    let valid = f.run(&["check", "--json"]);
    assert_eq!(valid.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<Value>(&valid.stdout).unwrap(),
        json!({"ok":true,"errors":[]})
    );
    f.create_task();
    let before = f.event_bytes();
    let output = f.run(&[
        "ask",
        "reviewer",
        "paper",
        "-m",
        "Review the outline.",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let answer: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(answer["text"], "Final answer\nSecond line");
    assert_eq!(answer["parsed"], Value::Null);
    assert_eq!(
        prompt(&f.0),
        "Inspect the paper design.\n\n# Encargo\n\nBuild a paper lighthouse.\n\n# Mensaje del director\n\nReview the outline."
    );
    let worktree = f.0.join(".agents/worktrees/paper");
    assert_eq!(
        fs::read_to_string(f.0.join("codex-cwd")).unwrap().trim(),
        worktree.to_str().unwrap()
    );
    let start = requests(&f.0)
        .into_iter()
        .find(|r| r["method"] == "thread/start")
        .unwrap();
    assert_eq!(start["params"]["cwd"], worktree.to_str().unwrap());
    assert_eq!(start["params"]["model"], "fixture-model");
    assert_eq!(start["params"]["sandbox"], "read-only");
    assert!(methods(&f.0).contains(&"turn/start".to_string()));
    assert!(
        f.event_bytes().starts_with(&before),
        "events must be append-only"
    );
    let events = f.events();
    let types: Vec<_> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
    assert_eq!(
        types,
        [
            "task_created",
            "brief_sent",
            "turn_started",
            "activity",
            "turn_finished"
        ]
    );
    for e in &events {
        assert_eq!(e["v"], 1);
        assert_eq!(e["task"], "paper");
        assert!(e["ts_ms"].is_i64());
    }
    assert_eq!(events[0]["team"]["director"], "director");
    assert_eq!(events[0]["worktree"], ".agents/worktrees/paper");
    assert_eq!(events[1]["from"], "director");
    assert_eq!(events[1]["to"], "reviewer");
    assert_eq!(events[1]["text"], "Review the outline.");
    assert_eq!(events[1]["resumed"], false);
    assert_eq!(events[2]["review_round"], 1);
    assert_eq!(events[2]["thread_id"], Value::Null);
    assert_eq!(events[3]["text"], "$ printf inspect");
    assert_eq!(events[4]["status"], "ok");
    assert_eq!(events[4]["thread_id"], "fixture-thread");
    assert_eq!(events[4]["text"], "Final answer");
    assert_eq!(
        events[4]["usage"],
        json!({"input_tokens":11,"cached_input_tokens":3,"output_tokens":7})
    );
    assert!(events[4]["duration_ms"].as_u64().is_some());
    let saved = f.0.join(".agents/state/paper");
    assert_eq!(
        fs::read_to_string(saved.join("01-reviewer.md")).unwrap(),
        "Final answer\nSecond line"
    );
    assert!(saved.join("01-reviewer.stderr").exists());
    assert!(saved.join("01-reviewer.jsonl").exists());
}

#[test]
fn review_limit_returns_three_without_launching_or_writing_and_force_can_resume() {
    let f = Fixture::new();
    f.create_task();
    let first = f.run(&["ask", "reviewer", "paper"]);
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    let before = f.event_bytes();
    fs::remove_file(f.0.join("requests.jsonl")).unwrap();
    let blocked = f.run(&["ask", "reviewer", "paper", "-m", "Check again."]);
    assert_eq!(blocked.status.code(), Some(3), "{blocked:?}");
    assert!(!blocked.stderr.is_empty());
    assert!(!f.0.join("requests.jsonl").exists());
    assert_eq!(f.event_bytes(), before);
    let forced = f.run(&["ask", "reviewer", "paper", "-m", "Check again.", "--force"]);
    assert_eq!(forced.status.code(), Some(0), "{forced:?}");
    assert_eq!(prompt(&f.0), "# Mensaje del director\n\nCheck again.");
    let resume = requests(&f.0)
        .into_iter()
        .find(|r| r["method"] == "thread/resume")
        .unwrap();
    assert_eq!(resume["params"]["threadId"], "fixture-thread");
    assert_eq!(resume["params"]["sandbox"], "read-only");
    let events = f.events();
    let starts: Vec<_> = events
        .iter()
        .filter(|e| e["type"] == "turn_started")
        .collect();
    assert_eq!(starts[1]["review_round"], 2);
    assert_eq!(starts[1]["thread_id"], "fixture-thread");
}
