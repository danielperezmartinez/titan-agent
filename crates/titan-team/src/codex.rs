//! One `codex exec` turn: its exact argv, running it without a shell with a
//! time limit, and reading its JSONL as it arrives (thread, usage, errors,
//! last answer, and `item.started` as live activity).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;
use titan_agent_core::team::{TEXT_LIMIT, Usage, clip};

use crate::config::{Role, TeamConfig};
use crate::process::{kill_tree, own_group};

/// Activity events written per turn, at most.
pub const ACTIVITY_LIMIT: usize = 200;

/// The executable: `TITAN_TEAM_CODEX`, or `codex` from the PATH.
pub fn executable() -> String {
    std::env::var("TITAN_TEAM_CODEX")
        .ok()
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| "codex".into())
}

/// Arguments after the executable, as fixed in ADR-0010.
pub fn argv(
    team: &TeamConfig,
    role: &Role,
    thread_id: Option<&str>,
    out_file: &Path,
    worktree: &Path,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["exec".into()];
    if thread_id.is_some() {
        args.push("resume".into());
    }
    args.extend([
        "--json".into(),
        "-o".into(),
        out_file.to_string_lossy().into_owned(),
        "-m".into(),
        role.model.clone(),
    ]);
    if let Some(effort) = &role.effort {
        args.extend(["-c".into(), format!("model_reasoning_effort={effort}")]);
    }
    for pair in team.codex_config.iter().chain(&role.config) {
        args.extend(["-c".into(), pair.clone()]);
    }
    match thread_id {
        Some(_) => args.extend(["-c".into(), format!("sandbox_mode=\"{}\"", role.sandbox)]),
        None => args.extend([
            "-s".into(),
            role.sandbox.clone(),
            "-C".into(),
            worktree.to_string_lossy().into_owned(),
        ]),
    }
    if let Some(schema) = &role.output_schema {
        args.extend([
            "--output-schema".into(),
            schema.to_string_lossy().into_owned(),
        ]);
    }
    if let Some(thread) = thread_id {
        args.push(thread.to_string());
    }
    args.push("-".into());
    args
}

/// What a turn produced.
#[derive(Debug, Default)]
pub struct TurnOutput {
    pub status: &'static str,
    pub stdout: String,
    pub stderr: String,
    pub thread_id: Option<String>,
    pub usage: Usage,
    pub errors: Vec<String>,
    /// Text of the last `agent_message`, used when there is no `-o` file.
    pub last_message: String,
}

/// Runs Codex with `prompt` on stdin. `on_activity` is called for each
/// `item.started` worth showing, while the turn runs.
pub fn run(
    program: &str,
    args: &[String],
    prompt: &str,
    cwd: &Path,
    limit: Duration,
    mut on_activity: impl FnMut(String),
) -> Result<TurnOutput, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    own_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("{program} no está disponible: {error}"))?;
    let mut stdin = child.stdin.take();
    let prompt = prompt.to_string();
    let writer = std::thread::spawn(move || {
        if let Some(stdin) = stdin.as_mut() {
            let _ = stdin.write_all(prompt.as_bytes());
        }
    });
    let stdout = child.stdout.take();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader = std::thread::spawn(move || {
        if let Some(stdout) = stdout {
            let mut lines = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                match lines.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });
    let mut stderr_pipe = child.stderr.take();
    let errors_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(stderr) = stderr_pipe.as_mut() {
            let _ = stderr.read_to_end(&mut bytes);
        }
        bytes
    });

    let deadline = Instant::now() + limit;
    let mut out = TurnOutput {
        status: "ok",
        ..TurnOutput::default()
    };
    let mut activities = 0;
    let mut timed_out = false;
    loop {
        // Checked on every line: a turn that never stops writing must not
        // outlive its limit.
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(bytes) => {
                let line = String::from_utf8_lossy(&bytes).into_owned();
                out.stdout.push_str(&line);
                if let Some(text) = read_line(&line, &mut out, cwd)
                    && activities < ACTIVITY_LIMIT
                {
                    activities += 1;
                    on_activity(text);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                timed_out = true;
                break;
            }
        }
    }
    let code = if timed_out {
        kill_tree(&mut child);
        None
    } else {
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Anything it left running would keep the pipes open.
                    kill_tree(&mut child);
                    break status.code();
                }
                Ok(None) if Instant::now() >= deadline => {
                    timed_out = true;
                    kill_tree(&mut child);
                    break None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => break Some(1),
            }
        }
    };
    let _ = writer.join();
    let _ = reader.join();
    out.stderr = String::from_utf8_lossy(&errors_reader.join().unwrap_or_default()).into_owned();
    if timed_out {
        out.status = "timeout";
    } else if code != Some(0) || !out.errors.is_empty() {
        out.status = "error";
    }
    Ok(out)
}

/// Reads one JSONL line into `out`; returns the activity text it carries.
fn read_line(line: &str, out: &mut TurnOutput, cwd: &Path) -> Option<String> {
    let event: Value = serde_json::from_str(line.trim()).ok()?;
    if out.thread_id.is_none()
        && let Some(thread) = event.get("thread_id").and_then(Value::as_str)
    {
        out.thread_id = Some(thread.to_string());
    }
    let item = event.get("item");
    match event.get("type").and_then(Value::as_str)? {
        "turn.completed" => {
            let usage = event.get("usage");
            let read = |key: &str| {
                usage
                    .and_then(|u| u.get(key))
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            };
            out.usage.input_tokens += read("input_tokens");
            out.usage.cached_input_tokens += read("cached_input_tokens");
            out.usage.output_tokens += read("output_tokens");
            None
        }
        "turn.failed" => {
            let message = event
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("turn.failed");
            out.errors.push(message.to_string());
            None
        }
        "error" => {
            let message = event
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("error");
            out.errors.push(message.to_string());
            None
        }
        "item.completed" => {
            if item.and_then(|i| i.get("type")).and_then(Value::as_str) == Some("agent_message")
                && let Some(text) = item.and_then(|i| i.get("text")).and_then(Value::as_str)
            {
                out.last_message = text.to_string();
            }
            None
        }
        "item.started" => activity(item?, cwd).map(|text| clip(&text, TEXT_LIMIT)),
        _ => None,
    }
}

/// `/usr/bin/bash -lc 'cargo test'` → `cargo test`: Codex runs every
/// command through a login shell, which says nothing to the reader.
fn unwrap_shell(command: &str) -> &str {
    let Some((program, rest)) = command.split_once(' ') else {
        return command;
    };
    let shell = program.rsplit('/').next().unwrap_or(program);
    let Some(script) = ["sh", "bash", "zsh"]
        .contains(&shell)
        .then(|| {
            rest.strip_prefix("-lc ")
                .or_else(|| rest.strip_prefix("-c "))
        })
        .flatten()
    else {
        return command;
    };
    let script = script.trim();
    for quote in ['\'', '"'] {
        if let Some(inner) = script
            .strip_prefix(quote)
            .and_then(|s| s.strip_suffix(quote))
            .filter(|inner| !inner.contains(quote))
        {
            return inner;
        }
    }
    script
}

/// The activity line of an `item.started`, if its kind is shown.
fn activity(item: &Value, cwd: &Path) -> Option<String> {
    match item.get("type").and_then(Value::as_str)? {
        "command_execution" => {
            let command = match item.get("command") {
                Some(Value::String(text)) => text.clone(),
                Some(Value::Array(parts)) => parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
                _ => String::new(),
            };
            Some(format!("$ {}", unwrap_shell(command.trim())))
        }
        "file_change" => {
            let root = format!("{}/", cwd.to_string_lossy().trim_end_matches('/'));
            let paths: Vec<String> = item
                .get("changes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|change| change.get("path").and_then(Value::as_str))
                .map(|path| path.strip_prefix(&root).unwrap_or(path).to_string())
                .collect();
            Some(format!("edita {}", paths.join(", ")))
        }
        kind @ ("mcp_tool_call" | "web_search") => Some(kind.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn items_become_short_activity_lines() {
        let cwd = Path::new("/w/task");
        let item =
            json!({"type":"file_change","changes":[{"path":"/w/task/src/a.rs"},{"path":"b.rs"}]});
        assert_eq!(activity(&item, cwd).unwrap(), "edita src/a.rs, b.rs");
        let item = json!({"type":"command_execution","command":["cargo","test"]});
        assert_eq!(activity(&item, cwd).unwrap(), "$ cargo test");
        assert_eq!(
            activity(&json!({"type":"web_search"}), cwd).unwrap(),
            "web_search"
        );
        assert!(activity(&json!({"type":"reasoning"}), cwd).is_none());
    }

    #[test]
    fn errors_usage_and_threads_are_collected() {
        let mut out = TurnOutput::default();
        let cwd = Path::new("/w");
        for line in [
            r#"{"type":"thread.started","thread_id":"t1"}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":5,"output_tokens":1}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":2,"cached_input_tokens":1}}"#,
            r#"{"type":"turn.failed","error":{"message":"boom"}}"#,
            r#"{"type":"error"}"#,
            "plain text",
        ] {
            assert!(read_line(line, &mut out, cwd).is_none());
        }
        assert_eq!(out.thread_id.as_deref(), Some("t1"));
        assert_eq!(out.usage.input_tokens, 7);
        assert_eq!(out.usage.cached_input_tokens, 1);
        assert_eq!(out.errors, ["boom", "error"]);
        let long = format!(
            r#"{{"type":"item.started","item":{{"type":"command_execution","command":"{}"}}}}"#,
            "x".repeat(300)
        );
        assert_eq!(
            read_line(&long, &mut out, cwd).unwrap().chars().count(),
            TEXT_LIMIT
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_turn_past_its_limit_is_killed_and_marked() {
        let args = vec![
            "-c".to_string(),
            "cat >/dev/null; sleep 5; echo late".to_string(),
        ];
        let out = run(
            "sh",
            &args,
            "hi",
            Path::new("."),
            Duration::from_millis(300),
            |_| {},
        )
        .unwrap();
        assert_eq!(out.status, "timeout");
        let args = vec![
            "-c".to_string(),
            "cat >/dev/null; while :; do echo '{}'; sleep 0.01; done".to_string(),
        ];
        let started = Instant::now();
        let out = run(
            "sh",
            &args,
            "hi",
            Path::new("."),
            Duration::from_millis(300),
            |_| {},
        )
        .unwrap();
        assert_eq!(out.status, "timeout");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "endless output must not beat the limit"
        );
        let args = vec!["-c".to_string(), "cat >/dev/null; exit 4".to_string()];
        let out = run(
            "sh",
            &args,
            "hi",
            Path::new("."),
            Duration::from_secs(5),
            |_| {},
        )
        .unwrap();
        assert_eq!(out.status, "error");
    }
}
