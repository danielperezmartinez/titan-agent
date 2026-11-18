//! Acceptance tests for the equipo-v7 public CLI contract.
#![cfg(unix)]

use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let f = Self(std::env::temp_dir().join(format!(
            "titan-equipo-v7-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir_all(f.0.join(".agents")).unwrap();
        f.git(&f.0, &["init", "-q", "-b", "main"]);
        f.git(&f.0, &["config", "user.name", "Paper Fixture"]);
        f.git(&f.0, &["config", "user.email", "paper@example.invalid"]);
        f.git(&f.0, &["commit", "-q", "--allow-empty", "-m", "start"]);
        fs::write(f.0.join("prompt.md"), "Build a paper kite.").unwrap();
        fs::write(f.0.join("brief.md"), "Build a paper kite.").unwrap();
        f.configure("", "");
        let script = r#"#!/bin/sh
cat > "$FIXTURE_ROOT/stdin"
touch "$FIXTURE_ROOT/codex-started"
out=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '-o' ]; then shift; out="$1"; fi
  shift
done
printf 'draft\n' > draft.txt
printf 'Paper response\n' > "$out"
printf '%s\n' '{"type":"thread.started","thread_id":"paper-thread"}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}'
"#;
        let path = f.0.join("codex");
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
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

    fn history(&self) -> Value {
        let state: Value =
            serde_json::from_slice(&fs::read(self.0.join(".agents/state/paper.json")).unwrap())
                .unwrap();
        state["history"].clone()
    }

    fn event_bytes(&self) -> Vec<u8> {
        // An absent log and an empty log both contain no events.
        match fs::read(self.0.join(".agents/state/paper/events.jsonl")) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => panic!("cannot read events: {e}"),
        }
    }

    fn events(&self) -> Vec<Value> {
        String::from_utf8(self.event_bytes())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn assert_rejected_brief(&self, args: &[&str], missing: &[&str]) {
        let events = self.event_bytes();
        let history = self.history();
        let _ = fs::remove_file(self.0.join("codex-started"));
        let out = self.run(args);
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("falta"), "{stderr}");
        for section in missing {
            assert!(
                stderr.contains(section),
                "missing section {section}: {stderr}"
            );
        }
        assert!(!self.0.join("codex-started").exists());
        assert_eq!(self.event_bytes(), events);
        assert_eq!(self.history(), history);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn c1_c2_c3_configuration_reports_contract_paths_and_codes() {
    let f = Fixture::new();
    for (project, role, path, code) in [
        ("", "checks = ['build']", "project.build_cmd", "missing_key"),
        (
            "",
            "checks = ['lint']",
            "roles.writer.checks",
            "unknown_value",
        ),
        (
            "build_cmd = 'true'",
            "checks = ['build']\nchecks_required = ['test']",
            "roles.writer.checks_required",
            "unknown_value",
        ),
        (
            "",
            "checks_required = 'yes'",
            "roles.writer.checks_required",
            "bad_type",
        ),
        (
            "",
            "checks_required = [1]",
            "roles.writer.checks_required",
            "bad_type",
        ),
        (
            "",
            "brief_sections = 'Contract'",
            "roles.writer.brief_sections",
            "bad_type",
        ),
        (
            "",
            "brief_sections = [1]",
            "roles.writer.brief_sections",
            "bad_type",
        ),
        (
            "",
            "brief_sections = ['']",
            "roles.writer.brief_sections",
            "bad_type",
        ),
        (
            "",
            "brief_sections = ['   ']",
            "roles.writer.brief_sections",
            "bad_type",
        ),
    ] {
        f.configure(project, role);
        let out = f.run(&["check", "--json"]);
        // C1/C2 explicitly specify exit 1, even though older acceptance tests use 2.
        assert_eq!(out.status.code(), Some(1), "{role}: {out:?}");
        let result: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(result["ok"], false);
        assert!(
            result["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["path"] == path && e["code"] == code),
            "{result}"
        );
    }
    f.configure(
        "build_cmd = 'true'",
        "checks = ['build']\nchecks_required = ['build']\nbrief_sections = ['Contract']",
    );
    let out = f.run(&["check", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!({"ok": true, "errors": []})
    );
}

#[test]
fn c1_build_runs_between_fmt_and_test_and_is_logged_and_committed() {
    let f = Fixture::new();
    let fmt = "test -f draft.txt && echo fmt > order.txt";
    let build = "test -f order.txt && echo build >> order.txt && echo build-output";
    let test = "echo test >> order.txt";
    f.configure(
        &format!("fmt_cmd = {fmt:?}\nbuild_cmd = {build:?}\ntest_cmd = {test:?}"),
        "checks = ['fmt', 'build', 'test']",
    );
    f.create_task();
    let out = f.run(&["ask", "writer", "paper"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let history = f.history();
    let checks = history[0]["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 3);
    for (check, (name, cmd)) in checks
        .iter()
        .zip([("fmt", fmt), ("build", build), ("test", test)])
    {
        assert_eq!(check["name"], name);
        assert_eq!(check["cmd"], cmd);
        assert_eq!(check["exit"], 0);
        assert!(check["duration_s"].as_f64().is_some_and(|v| v >= 0.0));
    }
    assert_eq!(
        f.git(&f.worktree(), &["show", "HEAD:order.txt"]),
        "fmt\nbuild\ntest\n"
    );
    let log = fs::read_to_string(f.0.join(".agents/state/paper/01-writer.checks.txt")).unwrap();
    assert!(log.contains("## build · exit 0"), "{log}");
    assert!(log.contains("build-output"), "{log}");
    let activities: Vec<_> = f
        .events()
        .into_iter()
        .filter(|e| e["type"] == "activity")
        .map(|e| e["text"].clone())
        .collect();
    assert_eq!(
        activities,
        vec![
            json!(format!("$ {fmt}")),
            json!(format!("$ {build}")),
            json!(format!("$ {test}"))
        ]
    );
}

#[test]
fn c2_required_failure_sets_error_runs_remaining_checks_and_commits() {
    for required in ["['build']", "true"] {
        let f = Fixture::new();
        f.configure(
            "build_cmd = 'echo build-failed; exit 7'\ntest_cmd = 'echo after > after.txt; exit 3'",
            &format!("checks = ['build', 'test']\nchecks_required = {required}"),
        );
        f.create_task();
        let before = f.git(&f.worktree(), &["rev-parse", "HEAD"]);
        let out = f.run(&["ask", "writer", "paper"]);
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        let history = f.history();
        assert_eq!(history[0]["status"], "error");
        assert_eq!(history[0]["checks"][0]["name"], "build");
        assert_eq!(history[0]["checks"][0]["exit"], 7);
        assert_eq!(history[0]["checks"][1]["name"], "test");
        assert_eq!(history[0]["checks"][1]["exit"], 3);
        for warning in [
            "comprobación build falló (exit 7)",
            "comprobación test falló (exit 3)",
        ] {
            assert!(
                history[0]["warnings"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(warning))
            );
        }
        assert_ne!(f.git(&f.worktree(), &["rev-parse", "HEAD"]), before);
        assert_eq!(f.git(&f.worktree(), &["show", "HEAD:draft.txt"]), "draft\n");
        assert_eq!(f.git(&f.worktree(), &["show", "HEAD:after.txt"]), "after\n");
        let events = f.events();
        let finished = events
            .iter()
            .find(|e| e["type"] == "turn_finished")
            .unwrap();
        assert_eq!(finished["status"], "error");
    }
}

#[test]
fn c2_optional_test_failure_warns_without_failing_the_turn() {
    for required in [
        "checks_required = ['build']",
        "checks_required = []",
        "checks_required = false",
        "",
    ] {
        let f = Fixture::new();
        f.configure(
            "build_cmd = 'true'\ntest_cmd = 'exit 3'",
            &format!("checks = ['build', 'test']\n{required}"),
        );
        f.create_task();
        let out = f.run(&["ask", "writer", "paper"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let history = f.history();
        assert_eq!(history[0]["status"], "ok");
        assert_eq!(history[0]["checks"][0]["exit"], 0);
        assert_eq!(history[0]["checks"][1]["exit"], 3);
        assert!(
            history[0]["warnings"]
                .as_array()
                .unwrap()
                .contains(&json!("comprobación test falló (exit 3)"))
        );
        let events = f.events();
        assert_eq!(
            events
                .iter()
                .find(|e| e["type"] == "turn_finished")
                .unwrap()["status"],
            "ok"
        );
    }
}

#[test]
fn c3_missing_sections_reject_first_turn_and_dry_run_without_side_effects() {
    for brief in [
        "Contract and Signatures appear only in a paragraph.\n",
        "```markdown\n# Contract\n## Signatures\n```\n",
        "    # Contract\n####### Signatures\n#Contract\n",
    ] {
        let f = Fixture::new();
        f.configure("", "brief_sections = ['Contract', 'Signatures']");
        fs::write(f.0.join("brief.md"), brief).unwrap();
        f.create_task();
        for args in [
            vec!["ask", "writer", "paper"],
            vec!["ask", "writer", "paper", "--dry-run"],
        ] {
            f.assert_rejected_brief(&args, &["Contract", "Signatures"]);
        }
    }
}

#[test]
fn c3_headings_allow_start_resume_skips_validation_and_new_rechecks() {
    let f = Fixture::new();
    f.configure(
        "",
        "brief_sections = ['Contract', 'Signatures', 'Visual criteria']",
    );
    fs::write(
        f.0.join("brief.md"),
        "   ## C. CONTRACT numbered  \n# Signatures\n###### VISUAL CRITERIA\n",
    )
    .unwrap();
    f.create_task();
    let first = f.run(&["ask", "writer", "paper"]);
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    assert!(f.0.join("codex-started").exists());
    // The contract names the copied brief as the source of section validation.
    fs::write(
        f.0.join(".agents/state/paper/encargo.md"),
        "No headings remain.\n",
    )
    .unwrap();
    fs::remove_file(f.0.join("codex-started")).unwrap();
    let resumed = f.run(&["ask", "writer", "paper", "-m", "Continue the kite."]);
    assert_eq!(resumed.status.code(), Some(0), "{resumed:?}");
    assert!(f.0.join("codex-started").exists());
    assert_eq!(f.history().as_array().unwrap().len(), 2);
    f.assert_rejected_brief(
        &["ask", "writer", "paper", "--new"],
        &["Contract", "Signatures", "Visual criteria"],
    );
    f.assert_rejected_brief(
        &["ask", "writer", "paper", "--new", "--dry-run"],
        &["Contract", "Signatures", "Visual criteria"],
    );
}

#[test]
fn c4_doc_names_build_command_and_brief_sections() {
    let f = Fixture::new();
    let out = f.run(&["doc"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let doc = String::from_utf8_lossy(&out.stdout);
    for key in ["build_cmd", "brief_sections"] {
        assert!(doc.contains(key), "documentation is missing {key}");
    }
}
