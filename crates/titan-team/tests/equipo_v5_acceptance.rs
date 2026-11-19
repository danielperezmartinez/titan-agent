//! Acceptance tests for the equipo-v5 public CLI contract.
#![cfg(unix)]

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

mod support;
use support::FakeCodex;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let f = Self(std::env::temp_dir().join(format!(
            "titan-equipo-v5-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir_all(f.0.join(".agents")).unwrap();
        f.git(&f.0, &["init", "-q", "-b", "main"]);
        f.git(&f.0, &["config", "user.name", "Paper Fixture"]);
        f.git(&f.0, &["config", "user.email", "paper@example.invalid"]);
        f.git(&f.0, &["commit", "-q", "--allow-empty", "-m", "start"]);
        fs::write(f.0.join("prompt.md"), "Inspect the paper kite.").unwrap();
        fs::write(f.0.join("brief.md"), "Build a paper kite.").unwrap();
        f.configure("", "");
        f.fake("", 0);
        f
    }

    fn configure(&self, project: &str, writer: &str) {
        fs::write(
            self.0.join(".agents/team.toml"),
            format!(
                r#"
version = 1
name = "paper"
[project]
autocommit = true
{project}
[roles.lead]
cli = "claude"
model = "lead-model"
launch = false
[roles.reviewer]
cli = "codex"
model = "review-model"
prompt_file = "prompt.md"
[roles.writer]
cli = "codex"
model = "write-model"
sandbox = "workspace-write"
prompt_file = "prompt.md"
{writer}
"#
            ),
        )
        .unwrap();
    }

    fn fake(&self, body: &str, exit: i32) {
        FakeCodex {
            body: body.into(),
            exit: (exit != 0).then_some(exit),
            ..FakeCodex::default()
        }
        .install(&self.0);
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_titan-team"))
            .current_dir(&self.0)
            .env("TITAN_TEAM_CODEX", self.0.join("codex"))
            .env("FIXTURE_ROOT", &self.0)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .args(args)
            .output()
            .unwrap()
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    fn create_task(&self) {
        let out = self.run(&["new", "paper", "--brief", "brief.md"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
    }

    fn worktree(&self) -> PathBuf {
        self.0.join(".agents/worktrees/paper")
    }
    fn artifact(&self, suffix: &str) -> PathBuf {
        self.0.join(format!(".agents/state/paper/01-{suffix}"))
    }
    fn history(&self) -> Value {
        let state: Value =
            serde_json::from_slice(&fs::read(self.0.join(".agents/state/paper.json")).unwrap())
                .unwrap();
        state["history"].clone()
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
    fn assert_committed(&self, entry: &Value, path: &str, content: &str) {
        let commit = entry["commit"].as_str().expect("turn must have a commit");
        assert!(!commit.is_empty());
        assert!(
            entry["changed_files"]
                .as_array()
                .unwrap()
                .contains(&json!(path))
        );
        assert_eq!(
            self.git(&self.worktree(), &["show", &format!("{commit}:{path}")]),
            content
        );
        assert_eq!(
            self.git(&self.worktree(), &["rev-parse", "HEAD"]).trim(),
            self.git(&self.worktree(), &["rev-parse", commit]).trim()
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_no_checks(entry: &Value) {
    assert!(
        entry
            .get("checks")
            .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)),
        "{entry}"
    );
}

#[test]
fn c1_validation_accepts_new_keys_and_collects_typed_and_semantic_errors() {
    let f = Fixture::new();
    f.configure(
        "fmt_cmd = 'true'\ntest_cmd = 'true'\ngate_cmd = 'true'",
        "checks = ['fmt', 'test', 'gate']\nchecks_required = true",
    );
    let valid = f.run(&["check", "--json"]);
    assert_eq!(valid.status.code(), Some(0), "{valid:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&valid.stdout).unwrap(),
        json!({"ok":true,"errors":[]})
    );

    let cases = [
        (
            "",
            "checks = ['fmt', 'test', 'gate', 'lint']",
            vec![
                ("project.fmt_cmd", "missing_key"),
                ("project.test_cmd", "missing_key"),
                ("project.gate_cmd", "missing_key"),
                ("roles.writer.checks", "unknown_value"),
            ],
        ),
        (
            "fmt_cmd = 7\ngate_cmd = false",
            "checks = 'fmt'\nchecks_required = 'yes'",
            vec![
                ("project.fmt_cmd", "bad_type"),
                ("project.gate_cmd", "bad_type"),
                ("roles.writer.checks", "bad_type"),
                ("roles.writer.checks_required", "bad_type"),
            ],
        ),
        (
            "fmt_cmd = 'true'",
            "checks = [7]",
            vec![("roles.writer.checks", "bad_type")],
        ),
    ];
    for (project, writer, expected) in cases {
        f.configure(project, writer);
        let out = f.run(&["check", "--json"]);
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        let result: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(result["ok"], false);
        for (path, code) in expected {
            assert!(
                result["errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["path"] == path && e["code"] == code),
                "missing {path}/{code}: {result}"
            );
        }
    }
    // Read-only includes both an explicit sandbox and the default sandbox.
    for sandbox in ["", "sandbox = 'read-only'\n"] {
        f.configure("gate_cmd = 'true'", "checks = []\nchecks_required = false");
        let config = f.0.join(".agents/team.toml");
        let text = fs::read_to_string(&config).unwrap().replace(
            "[roles.reviewer]\n",
            &format!("[roles.reviewer]\n{sandbox}checks = ['gate']\n"),
        );
        fs::write(config, text).unwrap();
        let out = f.run(&["check", "--json"]);
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        let result: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(
            result["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["path"] == "roles.reviewer.checks" && e["code"] == "unknown_value")
        );
    }
}

#[test]
fn c2_optional_checks_run_in_order_keep_output_and_commit_formatter_changes() {
    let f = Fixture::new();
    let fmt = "test -f src/draft.txt && printf 'formatted\\n' > src/draft.txt; i=1; while [ $i -le 405 ]; do echo fmt-line-$i; i=$((i+1)); done";
    let test = "grep -q formatted src/draft.txt && echo test-output; exit 3";
    let gate = "printf 'after failure\\n' > src/gate.txt; echo gate-output";
    f.configure(
        &format!("fmt_cmd = {fmt:?}\ntest_cmd = {test:?}\ngate_cmd = {gate:?}"),
        "checks = ['fmt', 'test', 'gate']",
    );
    f.fake("mkdir -p src; printf 'draft\\n' > src/draft.txt", 0);
    f.create_task();
    let out = f.run(&["ask", "writer", "paper", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let answer: Value = serde_json::from_slice(&out.stdout).unwrap();
    let entry = &f.history()[0];
    assert_eq!(entry["status"], "ok");
    assert_eq!(answer["checks"], entry["checks"]);
    assert!(entry.get("allowed").is_none());
    let checks = entry["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 3);
    for (check, (name, cmd, exit)) in
        checks
            .iter()
            .zip([("fmt", fmt, 0), ("test", test, 3), ("gate", gate, 0)])
    {
        assert_eq!(check["name"], name);
        assert_eq!(check["cmd"], cmd);
        assert_eq!(check["exit"], exit);
        assert!(check["duration_s"].as_f64().is_some_and(|v| v >= 0.0));
    }
    f.assert_committed(entry, "src/draft.txt", "formatted\n");
    f.assert_committed(entry, "src/gate.txt", "after failure\n");
    let log = fs::read_to_string(f.artifact("writer.checks.txt")).unwrap();
    let expected_fmt_tail: String = (6..=405).map(|i| format!("fmt-line-{i}\n")).collect();
    assert!(
        log.contains(&format!("## fmt · exit 0\n$ {fmt}\n{expected_fmt_tail}")),
        "{log}"
    );
    assert!(!log.lines().any(|l| l == "fmt-line-5"));
    assert!(log.contains(&format!("## test · exit 3\n$ {test}\ntest-output")));
    assert!(log.contains(&format!("## gate · exit 0\n$ {gate}\ngate-output")));
    let events = f.events();
    let activities: Vec<_> = events.iter().filter(|e| e["type"] == "activity").collect();
    assert_eq!(activities.len(), 3);
    for (activity, cmd) in activities.iter().zip([fmt, test, gate]) {
        assert_eq!(activity["role"], "writer");
        assert_eq!(activity["turn"], entry["turn"]);
        assert_eq!(activity["text"], format!("$ {cmd}"));
    }
    let finished = events
        .iter()
        .find(|e| e["type"] == "turn_finished")
        .unwrap();
    assert_eq!(finished["status"], "ok");
    for value in [entry, finished] {
        assert!(
            value["warnings"]
                .as_array()
                .unwrap()
                .contains(&json!("comprobación test falló (exit 3)"))
        );
    }
}

#[test]
fn c2_required_failure_sets_error_but_still_commits_and_runs_later_checks() {
    let f = Fixture::new();
    f.configure(
        "test_cmd = 'echo failed; exit 7'\ngate_cmd = 'echo completed > after.txt'",
        "checks = ['test', 'gate']\nchecks_required = true",
    );
    f.fake("echo draft > draft.txt", 0);
    f.create_task();
    let out = f.run(&["ask", "writer", "paper", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let answer: Value = serde_json::from_slice(&out.stdout).unwrap();
    let entry = &f.history()[0];
    assert_eq!(answer["status"], "error");
    assert_eq!(entry["status"], "error");
    assert_eq!(answer["checks"], entry["checks"]);
    assert_eq!(entry["checks"][0]["exit"], 7);
    assert_eq!(entry["checks"][1]["name"], "gate");
    assert_eq!(entry["checks"][1]["exit"], 0);
    f.assert_committed(entry, "draft.txt", "draft\n");
    f.assert_committed(entry, "after.txt", "completed\n");
    let events = f.events();
    let finished = events
        .iter()
        .find(|e| e["type"] == "turn_finished")
        .unwrap();
    assert_eq!(finished["status"], "error");
    assert_eq!(finished["commit"], entry["commit"]);
    for value in [entry, finished] {
        assert!(
            value["warnings"]
                .as_array()
                .unwrap()
                .contains(&json!("comprobación test falló (exit 7)"))
        );
    }
}

#[test]
fn c2_failed_codex_and_dry_run_do_not_execute_checks() {
    for dry in [false, true] {
        let f = Fixture::new();
        f.configure(
            "fmt_cmd = 'touch check-ran'",
            "checks = ['fmt']\nchecks_required = true",
        );
        f.fake("", 9);
        f.create_task();
        let out = if dry {
            f.run(&["ask", "writer", "paper", "--dry-run"])
        } else {
            f.run(&["ask", "writer", "paper", "--json"])
        };
        if dry {
            assert_eq!(out.status.code(), Some(0), "{out:?}");
            assert!(!f.0.join("codex-started").exists());
        } else {
            assert!(!out.status.success(), "{out:?}");
            let answer: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(answer["status"], "error");
            assert_no_checks(&answer);
        }
        assert!(!f.worktree().join("check-ran").exists());
        assert!(!f.artifact("writer.checks.txt").exists());
        for entry in f.history().as_array().unwrap() {
            assert_no_checks(entry);
        }
    }
}

#[test]
fn c3_gate_failure_reaches_codex_after_diff_and_tests_with_only_the_tail() {
    let f = Fixture::new();
    f.configure("test_cmd = 'echo test-before-gate'\ngate_cmd = 'i=1; while [ $i -le 155 ]; do echo gate-line-$i; i=$((i+1)); done; touch gate-ran; exit 5'", "");
    // The marker proves the gate ran in the worktree before Codex started.
    f.fake("test -f gate-ran || exit 8", 0);
    f.create_task();
    fs::write(f.worktree().join("diff.txt"), "original\n").unwrap();
    f.git(&f.worktree(), &["add", "diff.txt"]);
    f.git(&f.worktree(), &["commit", "-q", "-m", "prepare diff"]);
    let out = f.run(&[
        "ask", "reviewer", "paper", "--diff", "--test", "--gate", "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let prompt = support::prompt(&f.0);
    let diff = prompt.find("# Diff respecto a la base").unwrap();
    let tests = prompt.find("# Resultado de los tests (exit 0)").unwrap();
    let gate = prompt.find("# Compuerta (exit 5)").unwrap();
    assert!(diff < tests && tests < gate, "{prompt}");
    assert!(prompt[diff..tests].contains("diff.txt"));
    assert!(prompt[tests..gate].contains("test-before-gate"));
    let tail: Vec<_> = (6..=155).map(|i| format!("gate-line-{i}")).collect();
    // Blank-line framing is not specified; compare the output lines themselves.
    let lines: Vec<_> = prompt[gate..]
        .lines()
        .filter(|l| l.starts_with("gate-line-"))
        .collect();
    assert_eq!(lines, tail);
    assert!(prompt.trim_end().ends_with("gate-line-155"));
    let saved = fs::read_to_string(f.artifact("reviewer.gate.txt")).unwrap();
    let saved_lines: Vec<_> = saved
        .lines()
        .filter(|l| l.starts_with("gate-line-"))
        .collect();
    assert_eq!(saved_lines, tail);
}

#[test]
fn c3_missing_gate_is_side_effect_free_and_dry_run_only_describes_it() {
    let f = Fixture::new();
    f.create_task();
    let before = f.event_bytes();
    let out = f.run(&["ask", "reviewer", "paper", "--gate"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(!f.0.join("codex-started").exists());
    assert_eq!(f.event_bytes(), before);
    assert!(!f.artifact("reviewer.gate.txt").exists());
    f.configure("gate_cmd = 'touch gate-ran'", "checks = []");
    let dry = f.run(&["ask", "reviewer", "paper", "--gate", "--dry-run"]);
    assert_eq!(dry.status.code(), Some(0), "{dry:?}");
    let text = String::from_utf8_lossy(&dry.stdout);
    let prompt = text.split_once("--- prompt ---").unwrap().1;
    assert!(prompt.contains("# Compuerta"));
    assert!(prompt.contains("(no se ejecuta en --dry-run)"));
    assert!(!f.worktree().join("gate-ran").exists());
    assert!(!f.0.join("codex-started").exists());
    assert!(!f.artifact("reviewer.gate.txt").exists());
}

#[test]
fn c4_allow_patterns_suppress_only_matching_path_warnings_for_one_turn() {
    let f = Fixture::new();
    f.configure("", "allowed_paths = ['src/']\nforbidden_paths = ['docs/']");
    f.fake("mkdir -p docs; echo approved > docs/approved.md; echo extra > docs/extra.txt; echo blocked > docs/blocked.md", 0);
    f.create_task();
    let missing = f.run(&["ask", "writer", "paper", "--allow"]);
    assert_eq!(missing.status.code(), Some(2), "{missing:?}");
    let out = f.run(&[
        "ask",
        "writer",
        "paper",
        "--allow",
        "docs/approved.*",
        "--allow",
        "docs/*.txt",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let entry = &f.history()[0];
    assert_eq!(entry["allowed"], json!(["docs/approved.*", "docs/*.txt"]));
    let events = f.events();
    let finished = events
        .iter()
        .find(|e| e["type"] == "turn_finished")
        .unwrap();
    for value in [entry, finished] {
        let warnings = value["warnings"].as_array().unwrap();
        for path in ["docs/approved.md", "docs/extra.txt"] {
            for prefix in [
                "Fichero fuera de allowed_paths: ",
                "Fichero dentro de forbidden_paths: ",
            ] {
                assert!(
                    !warnings.contains(&json!(format!("{prefix}{path}"))),
                    "{warnings:?}"
                );
            }
        }
        assert!(warnings.contains(&json!("Fichero fuera de allowed_paths: docs/blocked.md")));
        assert!(warnings.contains(&json!("Fichero dentro de forbidden_paths: docs/blocked.md")));
    }
    f.fake("echo changed > docs/approved.md", 0);
    let next = f.run(&["ask", "writer", "paper", "-m", "Continue.", "--json"]);
    assert_eq!(next.status.code(), Some(0), "{next:?}");
    let history = f.history();
    let next = &history[1];
    assert!(next.get("allowed").is_none());
    assert!(next["warnings"].as_array().unwrap().contains(&json!(
        "Fichero dentro de forbidden_paths: docs/approved.md"
    )));
    assert!(
        next["warnings"]
            .as_array()
            .unwrap()
            .contains(&json!("Fichero fuera de allowed_paths: docs/approved.md"))
    );
}

#[test]
fn c5_doc_exposes_all_new_configuration_keys_and_flags() {
    let f = Fixture::new();
    let out = f.run(&["doc"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let doc = String::from_utf8_lossy(&out.stdout);
    for term in [
        "fmt_cmd",
        "gate_cmd",
        "checks",
        "checks_required",
        "--gate",
        "--allow",
    ] {
        assert!(doc.contains(term), "documentation is missing {term}");
    }
}
