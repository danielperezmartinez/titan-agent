//! Public CLI acceptance contract. Requires serde_json as a dev dependency.
#![cfg(unix)]

use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

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
        let fake = f.0.join("fake-codex");
        fs::write(
            &fake,
            r#"#!/bin/sh
printf '%s\n' "$@" > "$FIXTURE_ROOT/argv"
pwd > "$FIXTURE_ROOT/cwd"
cat > "$FIXTURE_ROOT/stdin"
output=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '-o' ]; then shift; output="$1"; fi
  shift
done
printf 'Final answer\nSecond line\n' > "$output"
printf '%s\n' 'ignored diagnostic line' \
  '{"type":"thread.started","thread_id":"fixture-thread"}' \
  '{"type":"item.started","item":{"type":"command_execution","command":"printf inspect"}}' \
  '{"type":"item.completed","item":{"type":"agent_message","text":"Fallback answer"}}' \
  '{"type":"turn.completed","usage":{"input_tokens":11,"cached_input_tokens":3,"output_tokens":7}}'
"#,
        )
        .unwrap();
        fs::set_permissions(fake, fs::Permissions::from_mode(0o755)).unwrap();
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
            .env("TITAN_TEAM_CODEX", self.0.join("fake-codex"))
            .env("FIXTURE_ROOT", &self.0)
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
    assert_eq!(answer["text"], "Final answer\nSecond line\n");
    assert_eq!(answer["parsed"], Value::Null);
    assert_eq!(
        fs::read_to_string(f.0.join("stdin")).unwrap(),
        "Inspect the paper design.\n\n# Encargo\n\nBuild a paper lighthouse.\n\n# Mensaje del director\n\nReview the outline."
    );
    let worktree = f.0.join(".agents/worktrees/paper");
    assert_eq!(
        PathBuf::from(fs::read_to_string(f.0.join("cwd")).unwrap().trim()),
        worktree
    );
    let args: Vec<String> = fs::read_to_string(f.0.join("argv"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(&args[..3], &["exec", "--json", "-o"]);
    assert_eq!(
        &args[4..],
        &[
            "-m",
            "fixture-model",
            "-c",
            "model_reasoning_effort=high",
            "-s",
            "read-only",
            "-C",
            worktree.to_str().unwrap(),
            "-"
        ]
    );
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
        "Final answer\nSecond line\n"
    );
    assert!(saved.join("01-reviewer.stderr").exists());
    assert!(
        fs::read_to_string(saved.join("01-reviewer.jsonl"))
            .unwrap()
            .contains("ignored diagnostic line")
    );
}

#[test]
fn review_limit_returns_three_without_launching_or_writing_and_force_can_resume() {
    let f = Fixture::new();
    f.create_task();
    let first = f.run(&["ask", "reviewer", "paper"]);
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    let before = f.event_bytes();
    fs::remove_file(f.0.join("argv")).unwrap();
    let blocked = f.run(&["ask", "reviewer", "paper", "-m", "Check again."]);
    assert_eq!(blocked.status.code(), Some(3), "{blocked:?}");
    assert!(!blocked.stderr.is_empty());
    assert!(!f.0.join("argv").exists());
    assert_eq!(f.event_bytes(), before);
    let forced = f.run(&["ask", "reviewer", "paper", "-m", "Check again.", "--force"]);
    assert_eq!(forced.status.code(), Some(0), "{forced:?}");
    assert_eq!(
        fs::read_to_string(f.0.join("stdin")).unwrap(),
        "# Mensaje del director\n\nCheck again."
    );
    let args = fs::read_to_string(f.0.join("argv")).unwrap();
    assert!(args.starts_with("exec\nresume\n--json\n-o\n"));
    assert!(args.contains("-c\nsandbox_mode=\"read-only\"\n"));
    assert!(args.ends_with("fixture-thread\n-\n"));
    let events = f.events();
    let starts: Vec<_> = events
        .iter()
        .filter(|e| e["type"] == "turn_started")
        .collect();
    assert_eq!(starts[1]["review_round"], 2);
    assert_eq!(starts[1]["thread_id"], "fixture-thread");
}
