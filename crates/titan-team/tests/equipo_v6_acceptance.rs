//! Acceptance tests for the equipo-v6 public command-line contract.
#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "titan-v6-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_titan-team"))
        .current_dir(root)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("TITAN_TEAM_CODEX", root.join("codex"))
        .env("FIXTURE_ROOT", root)
        .args(args)
        .output()
        .unwrap()
}

fn ok(out: Output) -> Output {
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    out
}

fn project() -> Temp {
    let f = Temp::new();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.name", "Paper Fixture"],
        vec!["config", "user.email", "paper@example.invalid"],
        vec!["commit", "-q", "--allow-empty", "-m", "start"],
    ] {
        ok(Command::new("git")
            .current_dir(&f.0)
            .args(args)
            .output()
            .unwrap());
    }
    fs::create_dir(f.0.join(".agents")).unwrap();
    fs::write(
        f.0.join(".agents/team.toml"),
        r#"version = 1
name = "paper"
[project]
autocommit = true
[roles.lead]
cli = "claude"
model = "lead-model"
launch = false
[roles.writer]
cli = "codex"
model = "write-model"
sandbox = "workspace-write"
prompt_file = "prompt.md"
"#,
    )
    .unwrap();
    fs::write(f.0.join("prompt.md"), "Build a paper kite.").unwrap();
    fs::write(f.0.join("brief.md"), "Prepare a paper kite.").unwrap();
    executable(
        &f.0.join("codex"),
        r#"#!/bin/sh
touch "$FIXTURE_ROOT/codex-started"
cat > "$FIXTURE_ROOT/stdin"
out=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = '-o' ]; then shift; out="$1"; fi
  shift
done
printf 'Paper response\n' > "$out"
printf '%s\n' '{"type":"thread.started","thread_id":"paper-thread"}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}'
"#,
    );
    ok(run(&f.0, &["new", "paper", "--brief", "brief.md"]));
    f
}

#[test]
fn c1_version_is_exact_without_a_project() {
    let f = Temp::new();
    for flag in ["--version", "-V"] {
        let out = ok(run(&f.0, &[flag]));
        assert_eq!(
            out.stdout,
            format!("titan-team {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
        );
        assert_eq!(fs::read_dir(&f.0).unwrap().count(), 0);
    }
}

#[test]
fn c2_message_file_aliases_reach_the_end_of_the_prompt_and_conflicts_fail() {
    let f = project();
    fs::write(
        f.0.join("director message.txt"),
        "Fold the blue wing.\nKeep the tail.",
    )
    .unwrap();
    for flags in [
        vec!["--message-file", "director message.txt"],
        vec!["--message-file=director message.txt"],
        vec!["-f", "director message.txt"],
        vec!["--file", "director message.txt"],
    ] {
        let mut args = vec!["ask", "writer", "paper"];
        args.extend(flags);
        ok(run(&f.0, &args));
        let prompt = fs::read_to_string(f.0.join("stdin")).unwrap();
        // Blank-line framing is unspecified; the heading and final message are contractual.
        let (_, message) = prompt.rsplit_once("# Mensaje del director\n").unwrap();
        assert_eq!(message.trim(), "Fold the blue wing.\nKeep the tail.");
    }
    for message_flag in ["-m", "--message"] {
        for file_flag in ["-f", "--file", "--message-file"] {
            let out = run(
                &f.0,
                &[
                    "ask",
                    "writer",
                    "paper",
                    message_flag,
                    "Fold.",
                    file_flag,
                    "director message.txt",
                ],
            );
            assert_eq!(out.status.code(), Some(2), "{out:?}");
        }
    }
    let out = run(
        &f.0,
        &["ask", "writer", "paper", "--message-file", "missing.txt"],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

fn history(root: &Path) -> Value {
    let state: Value =
        serde_json::from_slice(&fs::read(root.join(".agents/state/paper.json")).unwrap()).unwrap();
    state["history"].clone()
}

#[test]
fn c2_empty_messages_never_start_codex_or_append_state_on_either_turn() {
    let f = project();
    for continuing in [false, true] {
        if continuing {
            // Omitting a message on the first turn remains valid and opens the thread.
            ok(run(&f.0, &["ask", "writer", "paper"]));
            fs::remove_file(f.0.join("codex-started")).unwrap();
        }
        let events = f.0.join(".agents/state/paper/events.jsonl");
        let before_events = fs::read(&events).unwrap_or_default();
        let before_history = history(&f.0);
        for text in ["", "   ", " \n\r\t "] {
            fs::write(f.0.join("empty.txt"), text).unwrap();
            for flags in [
                vec!["-m", text],
                vec!["--message", text],
                vec!["-f", "empty.txt"],
                vec!["--file", "empty.txt"],
                vec!["--message-file", "empty.txt"],
                vec!["--message-file=empty.txt"],
            ] {
                let mut args = vec!["ask", "writer", "paper"];
                args.extend(flags);
                let out = run(&f.0, &args);
                assert_eq!(
                    out.status.code(),
                    Some(2),
                    "continuing={continuing}: {out:?}"
                );
                assert!(
                    String::from_utf8_lossy(&out.stderr).contains("vacío"),
                    "{out:?}"
                );
                assert!(!f.0.join("codex-started").exists());
                assert_eq!(fs::read(&events).unwrap_or_default(), before_events);
                assert_eq!(history(&f.0), before_history);
            }
        }
    }
}

#[test]
fn c3_wrapper_selects_binaries_preserves_arguments_and_handles_local_commands() {
    let f = Temp::new();
    fs::create_dir(f.0.join("bin")).unwrap();
    let script = f.0.join("bin/team");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bin/team"),
        &script,
    )
    .unwrap();
    let elsewhere = Temp::new();
    let fake = "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$ARG_LOG\"\nexit 17\n";
    let explicit = f.0.join("chosen tool");
    executable(&explicit, fake);
    let pinned = f.0.join(".agents/team-bin/titan-team");
    fs::create_dir_all(pinned.parent().unwrap()).unwrap();
    executable(&pinned, "#!/bin/sh\nexit 19\n");
    let cargo = f.0.join("bin/cargo");
    executable(&cargo, fake);
    let log = f.0.join("arguments");
    let invoke = |override_bin: Option<&Path>, args: &[&str]| {
        let mut cmd = Command::new("sh");
        cmd.arg(&script)
            .args(args)
            .current_dir(&elsewhere.0)
            .env_remove("TITAN_TEAM_BIN")
            .env("ARG_LOG", &log)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    f.0.join("bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            );
        if let Some(path) = override_bin {
            cmd.env("TITAN_TEAM_BIN", path);
        }
        cmd.output().unwrap()
    };
    let args = ["ask", "two words", "", "literal $x;*", "last"];
    let expected = "ask\ntwo words\n\nliteral $x;*\nlast\n";
    let out = invoke(Some(&explicit), &args);
    assert_eq!(out.status.code(), Some(17), "{out:?}");
    assert_eq!(fs::read_to_string(&log).unwrap(), expected);
    fs::remove_file(&log).unwrap();
    let nonexec = f.0.join("not executable");
    fs::write(&nonexec, fake).unwrap();
    for bad in [f.0.join("missing"), nonexec] {
        let out = invoke(Some(&bad), &args);
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("TITAN_TEAM_BIN"));
        assert!(!log.exists());
    }
    executable(&pinned, fake);
    for override_bin in [None, Some(Path::new(""))] {
        let out = invoke(override_bin, &args);
        assert_eq!(out.status.code(), Some(17), "{out:?}");
        assert_eq!(fs::read_to_string(&log).unwrap(), expected);
        fs::remove_file(&log).unwrap();
    }
    for _ in 0..2 {
        let out = ok(invoke(Some(&explicit), &["unpin"]));
        assert!(String::from_utf8_lossy(&out.stdout).contains("sin fijar"));
        assert!(!pinned.parent().unwrap().exists());
        assert!(!log.exists());
    }
    // This fixture is deliberately not a Git repository; pin must fail before creating team-bin.
    let out = invoke(Some(&explicit), &["pin", "missing-paper-revision"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("revisión"));
    assert!(!pinned.parent().unwrap().exists());
    assert!(!log.exists());
    // An invalid revision in a repository has the same failure contract.
    ok(Command::new("git")
        .current_dir(&f.0)
        .args(["init", "-q", "-b", "main"])
        .output()
        .unwrap());
    let out = invoke(Some(&explicit), &["pin", "missing-paper-revision"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("revisión"));
    assert!(!pinned.parent().unwrap().exists());
    assert!(!log.exists());
    for nonexecutable_pin in [false, true] {
        if nonexecutable_pin {
            fs::create_dir_all(pinned.parent().unwrap()).unwrap();
            fs::write(&pinned, fake).unwrap();
        }
        let out = invoke(None, &args);
        assert_eq!(out.status.code(), Some(17), "{out:?}");
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            format!(
                "run\n--quiet\n--release\n--manifest-path\n{}\n-p\ntitan-team\n--\n{expected}",
                f.0.join("Cargo.toml").display()
            )
        );
    }
}

#[test]
fn c5_help_and_documentation_advertise_the_public_options() {
    let f = project();
    for args in [vec!["--help"], vec![]] {
        let out = run(&f.0, &args);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        for term in ["--message-file", "--version"] {
            assert!(text.contains(term), "missing {term}: {text}");
        }
    }
    let out = run(&f.0, &["doc"]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("--message-file"), "{text}");
}
