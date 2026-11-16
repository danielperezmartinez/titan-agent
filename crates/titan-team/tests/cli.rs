//! End-to-end tests of titan-team on a throwaway repository laid out like
//! this one, with a fake `codex` (a shell script) first in PATH. Codex is
//! never called.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use titan_agent_core::team::events::{EventKind, parse_line};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// Prints the JSON events of a turn, writes the answer to `-o` and, unless
/// the sandbox is read-only, a file in its working folder.
const FAKE_CODEX: &str = r#"#!/bin/sh
out=""; prev=""; schema=""; resume=""; ro=""
for a in "$@"; do
  [ "$prev" = "-o" ] && out="$a"
  [ "$a" = "--output-schema" ] && schema=1
  [ "$a" = "resume" ] && resume=1
  [ "$a" = "read-only" ] && ro=1
  case "$a" in *read-only*) ro=1;; esac
  prev="$a"
done
cat > "$FAKE_LOG/prompt-$$"
echo "$@" >> "$FAKE_LOG/argv"
echo '{"type":"thread.started","thread_id":"th-fake"}'
echo '{"type":"item.started","item":{"type":"command_execution","command":"bash -lc '"'"'cargo test'"'"'"}}'
if [ -n "$schema" ]; then
  printf '%s' '{"verdict":"changes_requested","summary":"s","tests_assessment":"t","findings":[{"severity":"major","file":"src/a.rs","line":3,"issue":"i","suggestion":"s"}],"questions_for_director":[]}' > "$out"
else
  printf 'hecho\nresto' > "$out"
  [ -z "$ro" ] && echo "turno" >> "src/escrito.rs"
fi
echo '{"type":"turn.completed","usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":7}}'
"#;

struct Repo {
    _tmp: TempDir,
    base: PathBuf,
    bin: PathBuf,
    log: PathBuf,
    brief: PathBuf,
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sh(cwd: &Path, args: &[&str]) {
    let status = Command::new(args[0])
        .args(&args[1..])
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(status.status.success(), "{args:?}: {status:?}");
}

impl Repo {
    fn new(name: &str) -> Repo {
        let tmp = std::env::temp_dir().join(format!("titan-team-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let base = tmp.join("repo");
        for rel in [
            ".agents/team.toml",
            ".agents/prompts/disenador_tests.md",
            ".agents/prompts/mecanico.md",
            ".agents/prompts/revisor.md",
            ".agents/schemas/review.json",
        ] {
            let dest = base.join(rel);
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::copy(Path::new(ROOT).join(rel), dest).unwrap();
        }
        for rel in ["AGENTS.md", "CLAUDE.md", "docs/README.md", "src/main.rs"] {
            let dest = base.join(rel);
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::write(dest, format!("{rel}\n")).unwrap();
        }
        std::fs::write(
            base.join(".gitignore"),
            ".agents/state/\n.agents/worktrees/\n",
        )
        .unwrap();
        sh(&base, &["git", "init", "-q", "-b", "main"]);
        sh(&base, &["git", "add", "-A"]);
        sh(
            &base,
            &[
                "git",
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "commit",
                "-q",
                "-m",
                "init",
            ],
        );
        let bin = tmp.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("codex"), FAKE_CODEX).unwrap();
        std::fs::set_permissions(bin.join("codex"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let log = tmp.join("log");
        std::fs::create_dir_all(&log).unwrap();
        let brief = tmp.join("brief.md");
        std::fs::write(&brief, "# Encargo de prueba\n\nNada que hacer.\n").unwrap();
        Repo {
            _tmp: TempDir(tmp),
            base,
            bin,
            log,
            brief,
        }
    }

    fn team(&self, args: &[&str]) -> Output {
        let path = format!(
            "{}:{}",
            self.bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(env!("CARGO_BIN_EXE_titan-team"))
            .args(args)
            .current_dir(&self.base)
            .env("PATH", path)
            .env("FAKE_LOG", &self.log)
            .env("CLAUDE_CODE_SESSION_ID", "director-1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.team(args);
        assert!(
            out.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn events(&self) -> Vec<EventKind> {
        std::fs::read_to_string(self.base.join(".agents/state/events.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(parse_line)
            .map(|e| {
                assert_eq!(e.director.as_deref(), Some("director-1"));
                e.kind
            })
            .collect()
    }
}

#[test]
fn new_and_dry_run_ask_match_the_prototype() {
    let repo = Repo::new("dry");
    let brief = repo.brief.display().to_string();
    let out = repo.ok(&["new", "prueba", "--brief", &brief]);
    assert!(out.contains("Rama: encargo/prueba"), "{out}");
    let worktree = repo.base.join(".agents/worktrees/prueba");
    assert!(worktree.is_dir());

    let out = repo.ok(&["ask", "disenador_tests", "prueba", "--dry-run"]);
    assert!(out.contains("codex exec --json"), "{out}");
    assert!(out.contains("-m gpt-6.1-sol"));
    assert!(out.contains(&worktree.display().to_string()));
    assert!(out.contains("Nada que hacer."));
    assert!(out.contains("-c project_doc_max_bytes=0"));

    let out = repo.ok(&["ask", "revisor", "prueba", "--dry-run", "-m", "Revisa."]);
    assert!(
        out.contains("-s read-only") && out.contains("--output-schema"),
        "{out}"
    );

    // The worktree hides the director's instructions but keeps the code.
    for rel in ["AGENTS.md", "CLAUDE.md", "docs"] {
        assert!(!worktree.join(rel).exists(), "{rel}");
    }
    assert!(worktree.join("src/main.rs").is_file());
    assert!(worktree.join(".agents/team.toml").is_file());

    // A dry run writes no turn event.
    let events = repo.events();
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], EventKind::TaskCreated { roles, .. } if roles.len() == 4));
}

#[test]
fn a_turn_commits_warns_and_logs_its_events() {
    let repo = Repo::new("turn");
    let brief = repo.brief.display().to_string();
    repo.ok(&["new", "t1", "--brief", &brief]);
    let out = repo.team(&["ask", "disenador_tests", "t1"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("estado: ok"), "{stdout}");
    assert!(stdout.contains("commit: "), "{stdout}");
    assert!(
        stdout.contains("Fichero fuera de allowed_paths: src/escrito.rs"),
        "{stdout}"
    );
    let state: Value = serde_json::from_str(&repo.ok(&["show", "t1", "--json"])).unwrap();
    assert_eq!(state["roles"]["disenador_tests"]["thread_id"], "th-fake");
    assert_eq!(state["history"][0]["usage"]["output_tokens"], 7);
    assert_eq!(state["history"][0]["changed_files"][0], "src/escrito.rs");

    // Second turn continues the thread and needs a message.
    let out = repo.team(&["ask", "disenador_tests", "t1"]);
    assert_eq!(out.status.code(), Some(2));
    repo.ok(&["ask", "disenador_tests", "t1", "-m", "Añade un caso"]);
    let argv = std::fs::read_to_string(repo.log.join("argv")).unwrap();
    assert!(argv.lines().nth(1).unwrap().contains("resume"), "{argv}");
    assert!(argv.lines().nth(1).unwrap().contains("th-fake"), "{argv}");

    repo.ok(&["note", "t1", "-m", "acepto los tests"]);
    let events = repo.events();
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| match e {
            EventKind::TaskCreated { .. } => "created",
            EventKind::TurnStarted { .. } => "started",
            EventKind::Action { .. } => "action",
            EventKind::TurnFinished { .. } => "finished",
            EventKind::Review { .. } => "review",
            EventKind::Note { .. } => "note",
            EventKind::TaskClosed => "closed",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "created", "started", "action", "finished", "started", "action", "finished", "note"
        ]
    );
    match &events[3] {
        EventKind::TurnFinished {
            summary,
            thread_id,
            changed_files,
            warnings,
            usage,
            ..
        } => {
            assert_eq!(summary, "[ok] 1 fichero");
            assert_eq!(thread_id.as_deref(), Some("th-fake"));
            assert_eq!((*changed_files, *warnings), (1, 1));
            assert_eq!(usage.input_tokens, 100);
        }
        other => panic!("{other:?}"),
    }
    match &events[4] {
        EventKind::TurnStarted {
            resumed,
            message,
            turn,
            ..
        } => {
            assert!(*resumed);
            assert_eq!((message.as_str(), *turn), ("Añade un caso", 2));
        }
        other => panic!("{other:?}"),
    }
    match &events[2] {
        EventKind::Action {
            text, thread_id, ..
        } => {
            assert_eq!(text, "abre su hilo");
            assert_eq!(
                thread_id.as_deref(),
                Some("th-fake"),
                "the session is known mid-turn"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_review_is_parsed_and_limited_to_its_rounds() {
    let repo = Repo::new("review");
    let brief = repo.brief.display().to_string();
    repo.ok(&["new", "r1", "--brief", &brief]);
    let out = repo.ok(&["ask", "revisor", "r1", "-m", "Revisa el diff"]);
    assert!(out.contains("veredicto: changes_requested"), "{out}");
    assert!(out.contains("- [major] src/a.rs:3 — i → s"), "{out}");
    assert!(out.contains("ronda de revisión: 1/1"), "{out}");
    // read-only: nothing written, nothing committed.
    assert!(!out.contains("commit:"), "{out}");

    let out = repo.team(&["ask", "revisor", "r1", "--new", "-m", "Otra vez"]);
    assert_eq!(out.status.code(), Some(3), "the round limit is 1");

    let events = repo.events();
    assert!(events.iter().any(|e| matches!(
        e,
        EventKind::Review { round: 1, max_rounds: 1, verdict, findings: 1, .. } if verdict == "changes_requested"
    )));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EventKind::TurnStarted { round: Some(1), .. }))
    );
    assert!(events.iter().any(|e| matches!(
        e,
        EventKind::TurnFinished { summary, .. } if summary == "changes_requested · 1 hallazgo"
    )));

    repo.ok(&["close", "r1", "--delete-branch"]);
    assert!(!repo.base.join(".agents/worktrees/r1").exists());
    assert!(matches!(repo.events().last(), Some(EventKind::TaskClosed)));
    let out = repo.team(&["ask", "revisor", "r1", "-m", "x"]);
    assert_eq!(out.status.code(), Some(2), "a closed task takes no turns");
}

#[test]
fn the_director_and_claude_roles_are_never_launched() {
    let repo = Repo::new("director");
    let brief = repo.brief.display().to_string();
    repo.ok(&["new", "d1", "--brief", &brief]);
    let out = repo.team(&["ask", "director", "d1", "--dry-run"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("launch=false"));
    let out = repo.team(&["ask", "nadie", "d1"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn check_init_and_doc_let_an_agent_set_up_a_team() {
    let repo = Repo::new("setup");
    let out = repo.ok(&["check"]);
    assert!(out.starts_with("[ok]"), "{out}");

    // A new project: init, then check finds the missing prompts.
    let other = repo.base.parent().unwrap().join("otro");
    std::fs::create_dir_all(&other).unwrap();
    let other_text = other.display().to_string();
    let config = format!("{other_text}/.agents/team.toml");
    repo.ok(&["init", "--config", &other_text]);
    let out = repo.team(&["--config", &config, "check"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("prompt_file"));
    for prompt in ["tests", "revisor"] {
        let path = other.join(format!(".agents/prompts/{prompt}.md"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "Eres el REVISOR.\n").unwrap();
    }
    repo.ok(&["--config", &config, "check"]);
    let out = repo.team(&["init", "--config", &other_text]);
    assert_eq!(out.status.code(), Some(2), "init does not overwrite");

    // A typo is named with its line.
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(
        &config,
        text.replace("effort = \"high\"", "esfuerzo = \"high\""),
    )
    .unwrap();
    let out = repo.team(&["--config", &config, "roles"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr.contains("esfuerzo") && stderr.contains("línea"),
        "{stderr}"
    );

    let doc = repo.ok(&["doc"]);
    assert!(doc.contains("[roles.<nombre>]") && doc.contains("turn_finished"));
}
