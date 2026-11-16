//! `codex exec` (new thread) and `codex exec resume` (same thread): the
//! command line, a run that streams its JSON events, and what they mean.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use titan_agent_core::team::config::{RoleConfig, TeamConfig};
use titan_agent_core::team::events::Usage;

/// `-c` pairs from `[codex].config` and then the role's own `config`.
pub fn overrides(config: &TeamConfig, role: &RoleConfig) -> Vec<String> {
    config
        .codex
        .config
        .iter()
        .chain(role.config.iter())
        .flat_map(|pair| ["-c".to_string(), pair.clone()])
        .collect()
}

/// The argv of one turn. The prompt goes on stdin (`-`).
pub fn command(
    role: &RoleConfig,
    thread: Option<&str>,
    out_file: &Path,
    worktree: &Path,
    overrides: &[String],
    schema: Option<&Path>,
) -> Vec<String> {
    let mut cmd: Vec<String> = vec!["codex".into(), "exec".into()];
    if thread.is_some() {
        cmd.push("resume".into());
    }
    cmd.extend([
        "--json".into(),
        "-o".into(),
        out_file.display().to_string(),
        "-m".into(),
        role.model.clone(),
    ]);
    if let Some(effort) = &role.effort {
        cmd.extend(["-c".into(), format!("model_reasoning_effort={effort}")]);
    }
    cmd.extend(overrides.iter().cloned());
    match thread {
        Some(_) => cmd.extend(["-c".into(), format!("sandbox_mode=\"{}\"", role.sandbox())]),
        None => cmd.extend([
            "-s".into(),
            role.sandbox().to_string(),
            "-C".into(),
            worktree.display().to_string(),
        ]),
    }
    if let Some(schema) = schema {
        cmd.extend(["--output-schema".into(), schema.display().to_string()]);
    }
    if let Some(thread) = thread {
        cmd.push(thread.to_string());
    }
    cmd.push("-".into());
    cmd
}

/// Quotes argv for display (`--dry-run`).
pub fn shell_line(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            let plain = !arg.is_empty()
                && arg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./=:,@+%".contains(c));
            if plain {
                arg.clone()
            } else {
                format!("'{}'", arg.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What one turn produced.
#[derive(Debug, Default)]
pub struct Outcome {
    pub thread_id: Option<String>,
    pub usage: Usage,
    pub errors: Vec<String>,
    pub last_message: String,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub exit_ok: bool,
}

/// Folds `codex exec --json` lines, one at a time.
#[derive(Debug, Default)]
pub struct Parser {
    pub thread_id: Option<String>,
    pub usage: Usage,
    pub errors: Vec<String>,
    pub last_message: String,
}

impl Parser {
    /// Reads one line; returns what the role is doing now, if it says.
    pub fn line(&mut self, line: &str) -> Option<String> {
        let event: Value = serde_json::from_str(line.trim()).ok()?;
        if self.thread_id.is_none()
            && let Some(thread) = event.get("thread_id").and_then(Value::as_str)
        {
            self.thread_id = Some(thread.to_string());
        }
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "turn.completed" => {
                if let Some(usage) = event.get("usage") {
                    let get = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
                    self.usage.add(&Usage {
                        input_tokens: get("input_tokens"),
                        cached_input_tokens: get("cached_input_tokens"),
                        output_tokens: get("output_tokens"),
                    });
                }
                None
            }
            "turn.failed" => {
                let message = event
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("turn.failed");
                self.errors.push(message.to_string());
                None
            }
            "error" => {
                let message = event
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("error");
                self.errors.push(message.to_string());
                None
            }
            "item.started" | "item.completed" | "item.updated" => {
                let item = event.get("item")?;
                if kind == "item.completed"
                    && item.get("type").and_then(Value::as_str) == Some("agent_message")
                    && let Some(text) = item.get("text").and_then(Value::as_str)
                {
                    self.last_message = text.to_string();
                }
                action(item, kind == "item.started")
            }
            _ => None,
        }
    }
}

/// A short Spanish phrase for what an item of `codex exec` means.
pub fn action(item: &Value, started: bool) -> Option<String> {
    let kind = item.get("type").and_then(Value::as_str)?;
    let text = |k: &str| item.get(k).and_then(Value::as_str).unwrap_or("");
    let phrase = match kind {
        "command_execution" if started => format!("ejecuta {}", unwrap_shell(text("command"))),
        "file_change" => {
            let paths: Vec<&str> = item
                .get("changes")
                .and_then(Value::as_array)
                .map(|c| {
                    c.iter()
                        .filter_map(|c| c.get("path").and_then(Value::as_str))
                        .collect()
                })
                .unwrap_or_default();
            match paths.as_slice() {
                [] => "edita ficheros".into(),
                [one] => format!("edita {}", file_name(one)),
                many => format!("edita {} ficheros", many.len()),
            }
        }
        "reasoning" if !started => {
            let title = text("text")
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('*')
                .trim();
            if title.is_empty() {
                "razona".into()
            } else {
                format!("razona: {title}")
            }
        }
        "agent_message" if !started => "redacta la respuesta".into(),
        "mcp_tool_call" if started => format!("usa {}", text("tool")),
        "web_search" if started => "busca en la web".into(),
        _ => return None,
    };
    Some(titan_agent_core::team::events::label(&phrase))
}

/// `bash -lc 'cargo test'` → `cargo test`.
fn unwrap_shell(command: &str) -> String {
    let command = command.trim();
    for prefix in [
        "bash -lc ",
        "/bin/bash -lc ",
        "bash -c ",
        "sh -c ",
        "/bin/sh -c ",
    ] {
        if let Some(rest) = command.strip_prefix(prefix) {
            return rest.trim_matches(|c| c == '\'' || c == '"').to_string();
        }
    }
    command.to_string()
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Runs one turn: the prompt on stdin, stdout read line by line (what the
/// role does goes to `on_action`, with the thread id the first time Codex
/// reports it), stdout and stderr kept, killed after `timeout`.
pub fn run(
    argv: &[String],
    cwd: &Path,
    prompt: &str,
    timeout: Duration,
    mut on_action: impl FnMut(String, Option<String>) + Send + 'static,
) -> std::io::Result<Outcome> {
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let prompt = prompt.to_string();
    std::thread::spawn(move || {
        let _ = stdin.write_all(prompt.as_bytes());
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    let parser = Arc::new(Mutex::new((Parser::default(), String::new())));
    let reader = {
        let parser = Arc::clone(&parser);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let (action, new_thread) = {
                    let mut guard = parser.lock().unwrap_or_else(|p| p.into_inner());
                    guard.1.push_str(&line);
                    guard.1.push('\n');
                    let before = guard.0.thread_id.is_some();
                    let action = guard.0.line(&line);
                    let thread = guard.0.thread_id.clone().filter(|_| !before);
                    (action, thread)
                };
                match (action, new_thread) {
                    (Some(action), thread) => on_action(action, thread),
                    (None, Some(thread)) => on_action("abre su hilo".into(), Some(thread)),
                    (None, None) => {}
                }
            }
        })
    };
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let err_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _ = reader.join();
    let stderr = err_reader.join().unwrap_or_default();
    let (parser, stdout) = Arc::try_unwrap(parser)
        .map(|m| m.into_inner().unwrap_or_else(|p| p.into_inner()))
        .unwrap_or_default();
    Ok(Outcome {
        thread_id: parser.thread_id,
        usage: parser.usage,
        errors: parser.errors,
        last_message: parser.last_message,
        stdout,
        stderr,
        timed_out,
        exit_ok: status.is_some_and(|s| s.success()),
    })
}

/// The answer of the turn: the `-o` file when Codex wrote it, else the last
/// agent message.
pub fn answer(out_file: &PathBuf, outcome: &Outcome) -> String {
    std::fs::read_to_string(out_file).unwrap_or_else(|_| outcome.last_message.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn role(sandbox: &str) -> RoleConfig {
        let text = format!(
            "version = 1\n[roles.r]\ncli = \"codex\"\nmodel = \"m\"\neffort = \"high\"\nsandbox = \"{sandbox}\"\nprompt_file = \"p.md\"\nconfig = [\"b=2\"]\n[codex]\nconfig = [\"a=1\"]\n"
        );
        TeamConfig::parse(&text).unwrap().roles["r"].clone()
    }

    #[test]
    fn new_and_resumed_threads_get_the_same_overrides() {
        let config = TeamConfig::parse(
            "version = 1\n[codex]\nconfig = [\"a=1\"]\n[roles.r]\ncli = \"codex\"\nmodel = \"m\"\nprompt_file = \"p\"\nconfig = [\"b=2\"]\n",
        )
        .unwrap();
        let o = overrides(&config, &config.roles["r"]);
        assert_eq!(o, ["-c", "a=1", "-c", "b=2"]);
        let r = role("read-only");
        let out = PathBuf::from("out");
        let wt = PathBuf::from("wt");
        let fresh = command(&r, None, &out, &wt, &o, Some(Path::new("s.json")));
        assert_eq!(
            shell_line(&fresh),
            "codex exec --json -o out -m m -c model_reasoning_effort=high -c a=1 -c b=2 -s read-only -C wt --output-schema s.json -"
        );
        let resumed = command(&r, Some("th1"), &out, &wt, &o, None);
        assert_eq!(
            shell_line(&resumed),
            "codex exec resume --json -o out -m m -c model_reasoning_effort=high -c a=1 -c b=2 -c 'sandbox_mode=\"read-only\"' th1 -"
        );
    }

    #[test]
    fn parser_reads_thread_usage_errors_and_actions() {
        let mut p = Parser::default();
        let lines = [
            r#"{"type":"thread.started","thread_id":"th-1"}"#,
            r#"{"type":"item.started","item":{"type":"command_execution","command":"bash -lc 'cargo test -p core'"}}"#,
            r#"{"type":"item.completed","item":{"type":"reasoning","text":"**Plan the fix**\nmore"}}"#,
            r#"{"type":"item.completed","item":{"type":"file_change","changes":[{"path":"/w/src/lib.rs","kind":"update"}]}}"#,
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"hecho"}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":4,"output_tokens":3}}"#,
            r#"{"type":"error","message":"boom"}"#,
            "not json",
        ];
        let actions: Vec<String> = lines.iter().filter_map(|l| p.line(l)).collect();
        assert_eq!(
            actions,
            [
                "ejecuta cargo test -p core",
                "razona: Plan the fix",
                "edita lib.rs",
                "redacta la respuesta"
            ]
        );
        assert_eq!(p.thread_id.as_deref(), Some("th-1"));
        assert_eq!(p.usage.total(), 13);
        assert_eq!(p.errors, ["boom"]);
        assert_eq!(p.last_message, "hecho");
    }
}
