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
    let mut child = Command::new(codex)
        .arg("app-server")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
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
    assert_eq!(answers[5]["params"]["item"]["text"], "It's \"done\"\nreally");
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
