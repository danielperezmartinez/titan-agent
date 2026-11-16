//! Own end-to-end tests of titan-team (the acceptance tests are in
//! `phase3_acceptance.rs`): what must not run, the task lock and a turn
//! that is recorded even when its commit fails. Fake Codex, temporary repo.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Repo(PathBuf);

const TEAM: &str = r#"
version = 1
name = "kite"
[project]
test_cmd = "touch ran-tests"
autocommit = true
[flow]
review_role = "checker"
max_review_rounds = 0
[roles.lead]
cli = "claude"
model = "lead-model"
launch = false
[roles.checker]
cli = "codex"
model = "check-model"
prompt_file = "prompt.md"
[roles.writer]
cli = "codex"
model = "write-model"
sandbox = "workspace-write"
prompt_file = "prompt.md"
"#;

const FAKE: &str = r#"#!/bin/sh
cat > /dev/null
out=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '-o' ]; then shift; out="$1"; fi
  shift
done
echo made > made.txt
printf 'done\n' > "$out"
echo '{"type":"thread.started","thread_id":"t-1"}'
"#;

impl Repo {
    fn new() -> Repo {
        let root = std::env::temp_dir().join(format!(
            "titan-team-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".agents")).unwrap();
        let repo = Repo(root);
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "start",
        ]);
        fs::write(repo.0.join(".agents/team.toml"), TEAM).unwrap();
        fs::write(repo.0.join("prompt.md"), "Check the kite.").unwrap();
        fs::write(repo.0.join("brief.md"), "Fly a kite.").unwrap();
        fs::write(repo.0.join("codex"), FAKE).unwrap();
        fs::set_permissions(repo.0.join("codex"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            repo.run(&["new", "kite", "--brief", "brief.md"])
                .status
                .code(),
            Some(0)
        );
        repo
    }

    fn git(&self, args: &[&str]) {
        assert!(
            Command::new("git")
                .current_dir(&self.0)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_titan-team"))
            .current_dir(&self.0)
            .env("TITAN_TEAM_CODEX", self.0.join("codex"))
            .args(args)
            .output()
            .unwrap()
    }

    fn worktree(&self) -> PathBuf {
        self.0.join(".agents/worktrees/kite")
    }

    fn events(&self) -> Vec<Value> {
        fs::read_to_string(self.0.join(".agents/state/kite/events.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn dry_runs_and_refused_reviews_run_nothing() {
    let repo = Repo::new();
    let dry = repo.run(&["ask", "writer", "kite", "--test", "--dry-run"]);
    assert_eq!(dry.status.code(), Some(0), "{dry:?}");
    assert!(String::from_utf8_lossy(&dry.stdout).contains("no se ejecutan en --dry-run"));
    let refused = repo.run(&["ask", "checker", "kite", "--test"]);
    assert_eq!(refused.status.code(), Some(3));
    assert!(!repo.worktree().join("ran-tests").exists());
    assert!(!repo.worktree().join("made.txt").exists());
    let ignored = repo.run(&["close", "kite", "--dry-run"]);
    assert_eq!(ignored.status.code(), Some(2));
    assert!(
        repo.worktree().is_dir(),
        "an option close does not take must stop it"
    );
    assert_eq!(repo.events().len(), 1);
}

#[test]
fn a_busy_task_is_refused_at_once() {
    let repo = Repo::new();
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(repo.0.join(".agents/state/kite.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let busy = repo.run(&["ask", "writer", "kite"]);
    assert_eq!(busy.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&busy.stderr).contains("ocupado"));
    drop(lock);
    assert_eq!(repo.run(&["ask", "writer", "kite"]).status.code(), Some(0));
}

#[test]
fn a_turn_whose_commit_fails_is_still_recorded() {
    let repo = Repo::new();
    let hooks = repo.0.join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::write(hooks.join("pre-commit"), "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(hooks.join("pre-commit"), fs::Permissions::from_mode(0o755)).unwrap();
    repo.git(&["config", "core.hooksPath", hooks.to_str().unwrap()]);
    let out = repo.run(&["ask", "writer", "kite"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let events = repo.events();
    let finished = events
        .iter()
        .find(|e| e["type"] == "turn_finished")
        .unwrap();
    assert_eq!(finished["thread_id"], "t-1");
    assert!(finished["warnings"][0].as_str().unwrap().contains("commit"));
    let state: Value =
        serde_json::from_str(&fs::read_to_string(repo.0.join(".agents/state/kite.json")).unwrap())
            .unwrap();
    assert_eq!(state["roles"]["writer"]["thread_id"], "t-1");
    assert_eq!(state["history"].as_array().unwrap().len(), 1);
}
