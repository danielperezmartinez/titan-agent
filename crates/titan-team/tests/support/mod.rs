//! A fake `codex` for the tests of titan-team: `app-server daemon version`
//! answers from [`FakeCodex::daemon_version`], and `app-server` speaks the
//! app-server JSON-RPC over stdio (one JSON object per line) with canned
//! answers. Every line it receives is appended to `requests.jsonl` next to
//! the script; `turn/start` also touches `codex-started` there.
//!
//! It finds a request's `id` with a pattern, so it relies on what
//! titan-team promises (FORMAT.md): every request is one compact JSON line
//! that starts with `{"id":<n>,"method":"<method>"`.
#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// What the fake answers. `Default` is a writing turn that answers
/// `Paper response` on a new thread `paper-thread`.
#[derive(Debug, Clone)]
pub struct FakeCodex {
    /// `thread.id` of `thread/start`, `thread/read` and `thread/resume`.
    pub thread_id: String,
    /// `turn.id` of `turn/start` (and of its notifications).
    pub turn_id: String,
    /// `thread.status.type` of `thread/read`: `idle`, `active`, `notLoaded`.
    pub thread_status: String,
    /// `data` of `thread/turns/list` (newest first), as Codex sends it.
    pub turns: Value,
    /// When set, `thread/resume` answers a JSON-RPC error with this message.
    pub resume_error: Option<String>,
    /// Items sent as `item/started` notifications once the turn starts.
    pub activity: Vec<Value>,
    /// Shell run while the turn runs, in the server's working folder (the
    /// worktree); its stdout is discarded.
    pub body: String,
    /// When set, the server exits with this code right after `body`,
    /// without answering anything else.
    pub exit: Option<i32>,
    /// Text of the final `agentMessage`.
    pub answer: String,
    /// `tokenUsage.last` of the one `thread/tokenUsage/updated`:
    /// (input, cached input, output).
    pub usage: (u64, u64, u64),
    /// `turn.status` of `turn/completed`: `completed`, `failed`,
    /// `interrupted`.
    pub status: String,
    /// The turn never completes on its own: only `turn/interrupt` ends it
    /// (with `interrupted`).
    pub hang: bool,
    /// Standard output of `codex app-server daemon version`.
    pub daemon_version: String,
}

impl Default for FakeCodex {
    fn default() -> Self {
        FakeCodex {
            thread_id: "paper-thread".into(),
            turn_id: "paper-turn".into(),
            thread_status: "idle".into(),
            turns: json!([]),
            resume_error: None,
            activity: Vec::new(),
            body: String::new(),
            exit: None,
            answer: "Paper response".into(),
            usage: (1, 0, 1),
            status: "completed".into(),
            hang: false,
            daemon_version: r#"{"status":"stopped"}"#.into(),
        }
    }
}

/// `'…'` for sh, whatever the text holds.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

impl FakeCodex {
    /// Writes the executable `codex` into `dir` and returns its path.
    pub fn install(&self, dir: &Path) -> PathBuf {
        let thread = &self.thread_id;
        let turn = &self.turn_id;
        let thread_json = |status: &str| {
            json!({"thread": {"id": thread, "status": {"type": status}}}).to_string()
        };
        let mut turn_lines = vec![
            json!({"method": "turn/started", "params": {"threadId": thread, "turn": {"id": turn, "status": "inProgress"}}}),
        ];
        for item in &self.activity {
            turn_lines.push(json!({"method": "item/started", "params": {"threadId": thread, "turnId": turn, "item": item}}));
        }
        let mut end_lines = vec![
            json!({"method": "item/completed", "params": {"threadId": thread, "turnId": turn, "item": {"type": "agentMessage", "id": "msg-1", "text": self.answer, "phase": "final_answer"}}}),
            json!({"method": "thread/tokenUsage/updated", "params": {"threadId": thread, "turnId": turn, "tokenUsage": {"last": {"inputTokens": self.usage.0, "cachedInputTokens": self.usage.1, "outputTokens": self.usage.2}, "total": {"inputTokens": self.usage.0, "cachedInputTokens": self.usage.1, "outputTokens": self.usage.2}}}}),
        ];
        if !self.hang {
            end_lines.push(json!({"method": "turn/completed", "params": {"threadId": thread, "turn": {"id": turn, "status": self.status, "error": if self.status == "completed" { Value::Null } else { json!({"message": format!("turn {}", self.status)}) }}}}));
        }
        let print = |lines: &[Value]| {
            lines
                .iter()
                .map(|line| format!("    printf '%s\\n' {}\n", quote(&line.to_string())))
                .collect::<String>()
        };
        let interrupted = json!({"method": "turn/completed", "params": {"threadId": thread, "turn": {"id": turn, "status": "interrupted", "error": null}}});
        let resume = match &self.resume_error {
            Some(message) => format!(
                "printf '{{\"id\":%s,\"error\":{{\"code\":-32600,\"message\":%s}}}}\\n' \"$id\" {}",
                quote(&Value::String(message.clone()).to_string())
            ),
            None => format!(
                "printf '{{\"id\":%s,\"result\":%s}}\\n' \"$id\" {}",
                quote(&thread_json("idle"))
            ),
        };
        let exit = self
            .exit
            .map(|code| format!("    exit {code}\n"))
            .unwrap_or_default();
        let script = format!(
            r#"#!/bin/sh
dir=$(cd "$(dirname "$0")" && pwd)
if [ "$1" = app-server ] && [ "$2" = daemon ]; then
  printf '%s\n' {daemon}
  exit 0
fi
[ "$1" = app-server ] || exit 64
printf '%s\n' "$@" > "$dir/codex-args"
printf '%s' "${{CARGO_TARGET_DIR-unset}}" > "$dir/codex-target"
pwd > "$dir/codex-cwd"
answer() {{ printf '{{"id":%s,"result":%s}}\n' "$id" "$1"; }}
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$dir/requests.jsonl"
  id=$(printf '%s\n' "$line" | sed -n 's/^{{"id":\([0-9][0-9]*\),.*/\1/p')
  [ -n "$id" ] || continue
  case "$line" in
  *'"method":"thread/start"'*) answer {started} ;;
  *'"method":"thread/read"'*) answer {read} ;;
  *'"method":"thread/turns/list"'*) answer {turns} ;;
  *'"method":"thread/resume"'*) {resume} ;;
  *'"method":"turn/start"'*)
    touch "$dir/codex-started"
    answer {turn_started}
{turn_lines}    ( {body}
    ) > /dev/null
{exit}{end_lines}    ;;
  *'"method":"turn/interrupt"'*)
    answer '{{}}'
    printf '%s\n' {interrupted}
    ;;
  *'"method":"thread/unsubscribe"'*) answer '{{"status":"unsubscribed"}}' ;;
  *) answer '{{}}' ;;
  esac
done
"#,
            daemon = quote(&self.daemon_version),
            started = quote(&thread_json("idle")),
            read = quote(&thread_json(&self.thread_status)),
            turns = quote(&json!({"data": self.turns, "nextCursor": null}).to_string()),
            turn_started =
                quote(&json!({"turn": {"id": turn, "status": "inProgress"}}).to_string()),
            turn_lines = print(&turn_lines),
            body = if self.body.trim().is_empty() {
                ":"
            } else {
                &self.body
            },
            end_lines = print(&end_lines),
            interrupted = quote(&interrupted.to_string()),
        );
        let path = dir.join("codex");
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

/// Every request the fake received, in order (`dir/requests.jsonl`).
pub fn requests(dir: &Path) -> Vec<Value> {
    fs::read_to_string(dir.join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// The methods of [`requests`], in order.
pub fn methods(dir: &Path) -> Vec<String> {
    requests(dir)
        .iter()
        .filter_map(|r| r["method"].as_str().map(str::to_string))
        .collect()
}

/// The prompt of the last `turn/start`: the text of its first input.
pub fn prompt(dir: &Path) -> String {
    requests(dir)
        .iter()
        .rev()
        .find(|r| r["method"] == "turn/start")
        .and_then(|r| r["params"]["input"][0]["text"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// A finished turn of `thread/turns/list` with one user message.
pub fn user_turn(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "status": "completed",
        "items": [
            {"type": "userMessage", "id": format!("{id}-user"), "content": [{"type": "text", "text": text}]},
            {"type": "agentMessage", "id": format!("{id}-agent"), "text": "ok"}
        ]
    })
}
