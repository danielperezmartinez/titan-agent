//! One turn of a Codex role through the app-server (ADR-0013): the thread
//! settings, the turn's requests and how its notifications become the
//! answer, the usage and the live activity. [`prepare`] connects and reads
//! the thread before the turn is announced (busy, the user's turns);
//! [`run`] starts or resumes the thread and runs the turn.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use titan_agent_core::team::{STATUS_BUSY, TEXT_LIMIT, Usage, clip};

use crate::appserver::{Client, Closed, SETUP_LIMIT, initialize_params, request_line};
use crate::config::{Role, TeamConfig};

/// Activity events written per turn, at most.
pub const ACTIVITY_LIMIT: usize = 200;
/// How long to wait for `turn/completed` after `turn/interrupt`.
const INTERRUPT_WAIT: Duration = Duration::from_secs(10);
/// Turns asked to `thread/turns/list` to find the user's turns.
pub const TURNS_LIMIT: u64 = 50;

/// The executable: `TITAN_TEAM_CODEX`, or `codex` from the PATH.
pub fn executable() -> String {
    std::env::var("TITAN_TEAM_CODEX")
        .ok()
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| "codex".into())
}

/// `5` → 5, `"high"` → "high", `[1]` → [1]: a `-c` value read as TOML, or
/// the text itself when it is not TOML.
fn toml_value(text: &str) -> Value {
    toml::from_str::<toml::Table>(&format!("v = {text}"))
        .ok()
        .and_then(|mut table| table.remove("v"))
        .and_then(|value| serde_json::to_value(value).ok())
        .unwrap_or_else(|| Value::String(text.to_string()))
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The `config` of `thread/start` and `thread/resume` (FORMAT.md): the
/// effort, `[codex].config` and the role's `config`, and the team's build
/// folder for a writing role.
pub fn config_object(team: &TeamConfig, role: &Role) -> Map<String, Value> {
    let mut config = Map::new();
    if let Some(effort) = &role.effort {
        config.insert("model_reasoning_effort".into(), json!(effort));
    }
    for pair in team.codex_config.iter().chain(&role.config) {
        if let Some((key, value)) = pair.split_once('=') {
            config.insert(key.trim().to_string(), toml_value(value.trim()));
        }
    }
    // A writing role builds in the team's shared folder too, which lies
    // outside its worktree: its sandbox must let it write there, and its
    // commands must use it.
    if let Some(dir) = team
        .project
        .target_dir
        .as_ref()
        .filter(|_| role.sandbox == "workspace-write")
    {
        let dir = absolute(dir).to_string_lossy().into_owned();
        config.insert(
            "sandbox_workspace_write.writable_roots".into(),
            json!([dir]),
        );
        config.insert(
            "shell_environment_policy.set".into(),
            json!({"CARGO_TARGET_DIR": dir}),
        );
    }
    config
}

/// `thread/start` (new thread) or `thread/resume` and their params.
pub fn thread_request(
    team: &TeamConfig,
    role: &Role,
    thread: Option<&str>,
    worktree: &Path,
) -> (&'static str, Value) {
    let mut params = json!({
        "model": role.model,
        "sandbox": role.sandbox,
        "approvalPolicy": "never",
        "config": config_object(team, role),
    });
    match thread {
        Some(id) => {
            params["threadId"] = json!(id);
            ("thread/resume", params)
        }
        None => {
            params["cwd"] = json!(absolute(worktree).to_string_lossy());
            ("thread/start", params)
        }
    }
}

/// The params of `turn/start`.
pub fn turn_params(thread: &str, prompt: &str, schema: Option<&Value>) -> Value {
    let mut params = json!({
        "threadId": thread,
        "input": [{"type": "text", "text": prompt}],
    });
    if let Some(schema) = schema {
        params["outputSchema"] = schema.clone();
    }
    params
}

/// The role's output schema, read as JSON.
pub fn read_schema(role: &Role) -> Result<Option<Value>, String> {
    let Some(path) = &role.output_schema else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("No se pudo leer {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| format!("{} no es JSON: {error}", path.display()))
}

/// The requests `--dry-run` shows: `initialize`, the thread request and
/// `turn/start` (the thread id of a new thread is not known yet).
pub fn dry_run_lines(
    team: &TeamConfig,
    role: &Role,
    thread: Option<&str>,
    worktree: &Path,
    prompt: &str,
    schema: Option<&Value>,
) -> Vec<String> {
    let (method, params) = thread_request(team, role, thread, worktree);
    vec![
        request_line(1, "initialize", &initialize_params()),
        request_line(2, method, &params),
        request_line(
            3,
            "turn/start",
            &turn_params(thread.unwrap_or("<hilo nuevo>"), prompt, schema),
        ),
    ]
}

/// What a turn produced.
#[derive(Debug, Default)]
pub struct TurnOutput {
    pub status: &'static str,
    /// The server's messages for the thread, one JSON per line.
    pub log: String,
    pub stderr: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub usage: Usage,
    pub errors: Vec<String>,
    /// Text of the last `agentMessage`.
    pub last_message: String,
}

/// Whether Codex refused a thread because another process writes it.
pub fn is_busy_message(text: &str) -> bool {
    text.to_lowercase().contains("already has an active writer")
}

/// A connection made before the turn is announced, with what it learned
/// of the thread.
pub struct Prepared {
    client: Option<Client>,
    /// The thread has a turn in progress: nothing may run.
    pub busy: bool,
    /// `data` of `thread/turns/list` (newest first) of a continued thread.
    pub turns: Vec<Value>,
    error: Option<String>,
}

/// Connects and, for a continued thread, reads its state and its turns.
pub fn prepare(
    program: &str,
    cwd: &Path,
    env: &[(&str, PathBuf)],
    thread: Option<&str>,
) -> Prepared {
    let mut prepared = Prepared {
        client: None,
        busy: false,
        turns: Vec::new(),
        error: None,
    };
    let mut client = match Client::connect(program, cwd, env) {
        Ok(client) => client,
        Err(error) => {
            prepared.error = Some(error);
            return prepared;
        }
    };
    if let Some(id) = thread {
        let deadline = Instant::now() + SETUP_LIMIT;
        match client.request("thread/read", &json!({"threadId": id}), deadline) {
            Ok(Ok(result)) => {
                prepared.busy = result
                    .pointer("/thread/status/type")
                    .and_then(Value::as_str)
                    == Some("active");
            }
            Ok(Err(error)) => prepared.error = Some(format!("thread/read: {error}")),
            Err(Closed(error)) => prepared.error = Some(error),
        }
        if !prepared.busy && prepared.error.is_none() {
            let params = json!({"threadId": id, "limit": TURNS_LIMIT});
            match client.request("thread/turns/list", &params, deadline) {
                Ok(Ok(result)) => {
                    prepared.turns = result
                        .get("data")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                }
                Ok(Err(error)) => prepared.error = Some(format!("thread/turns/list: {error}")),
                Err(Closed(error)) => prepared.error = Some(error),
            }
        }
    }
    prepared.client = Some(client);
    prepared
}

/// What [`run`] needs besides the connection.
pub struct TurnRequest<'a> {
    pub team: &'a TeamConfig,
    pub role: &'a Role,
    pub thread: Option<&'a str>,
    pub worktree: &'a Path,
    pub prompt: &'a str,
    pub limit: Duration,
}

/// Starts or resumes the thread and runs one turn. `on_activity` is called
/// for each `item/started` worth showing, while the turn runs.
pub fn run(
    prepared: Prepared,
    request: &TurnRequest,
    mut on_activity: impl FnMut(String),
) -> TurnOutput {
    let mut out = TurnOutput {
        status: "ok",
        thread_id: request.thread.map(str::to_string),
        ..TurnOutput::default()
    };
    let busy = prepared.busy;
    let Some(mut client) = prepared.client else {
        out.status = "error";
        out.errors.push(
            prepared
                .error
                .unwrap_or_else(|| "sin conexión con Codex".into()),
        );
        out.stderr = out.errors.join("\n");
        return out;
    };
    if busy {
        out.status = STATUS_BUSY;
    } else if let Some(error) = prepared.error {
        out.status = "error";
        out.errors.push(error);
    } else {
        turn(&mut client, request, &mut out, &mut on_activity);
    }
    if let Some(thread) = out.thread_id.clone().filter(|_| !busy) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let _ = client.request("thread/unsubscribe", &json!({"threadId": thread}), deadline);
    }
    let server = client.close();
    let mut stderr = server;
    for error in &out.errors {
        if !stderr.is_empty() && !stderr.ends_with('\n') {
            stderr.push('\n');
        }
        stderr.push_str(error);
        stderr.push('\n');
    }
    out.stderr = stderr;
    out
}

/// The thread request and the turn itself; leaves the result in `out`.
fn turn(
    client: &mut Client,
    request: &TurnRequest,
    out: &mut TurnOutput,
    on_activity: &mut impl FnMut(String),
) {
    let fail = |out: &mut TurnOutput, error: String| {
        out.status = if is_busy_message(&error) {
            STATUS_BUSY
        } else {
            "error"
        };
        out.errors.push(error);
    };
    let schema = match read_schema(request.role) {
        Ok(schema) => schema,
        Err(error) => return fail(out, error),
    };
    let setup = Instant::now() + SETUP_LIMIT;
    let (method, params) =
        thread_request(request.team, request.role, request.thread, request.worktree);
    match client.request(method, &params, setup) {
        Ok(Ok(result)) => {
            if let Some(id) = result.pointer("/thread/id").and_then(Value::as_str) {
                out.thread_id = Some(id.to_string());
            }
        }
        Ok(Err(error)) => return fail(out, format!("{method}: {error}")),
        Err(Closed(error)) => return fail(out, error),
    }
    let Some(thread) = out.thread_id.clone() else {
        return fail(out, format!("{method} no devolvió el hilo"));
    };
    let params = turn_params(&thread, request.prompt, schema.as_ref());
    match client.request("turn/start", &params, setup) {
        Ok(Ok(result)) => {
            out.turn_id = result
                .pointer("/turn/id")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        Ok(Err(error)) => return fail(out, format!("turn/start: {error}")),
        Err(Closed(error)) => return fail(out, error),
    }
    let Some(turn) = out.turn_id.clone() else {
        return fail(out, "turn/start no devolvió el turno".into());
    };
    let deadline = Instant::now() + request.limit;
    let mut activities = 0;
    let mut interrupted_at: Option<Instant> = None;
    loop {
        let until = interrupted_at.map_or(deadline, |at| at + INTERRUPT_WAIT);
        let message = match client.next(until) {
            Ok(Some(message)) => message,
            Ok(None) if interrupted_at.is_none() => {
                // Out of time: ask Codex to stop the turn and wait a little
                // for it to say so.
                let params = json!({"threadId": thread, "turnId": turn});
                let _ = client.request("turn/interrupt", &params, Instant::now() + INTERRUPT_WAIT);
                interrupted_at = Some(Instant::now());
                continue;
            }
            Ok(None) => break,
            Err(Closed(error)) => {
                if interrupted_at.is_none() {
                    fail(out, error);
                }
                break;
            }
        };
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            continue;
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let message_thread = params.get("threadId").and_then(Value::as_str);
        if message_thread.is_some_and(|t| t != thread) {
            continue;
        }
        out.log.push_str(&message.to_string());
        out.log.push('\n');
        let message_turn = params
            .get("turnId")
            .and_then(Value::as_str)
            .or_else(|| params.pointer("/turn/id").and_then(Value::as_str));
        if message_thread.is_none()
            || message_turn != Some(turn.as_str())
            || message.get("id").is_some()
        {
            continue;
        }
        match method {
            "item/started" => {
                if let Some(text) = params
                    .get("item")
                    .and_then(|item| activity(item, request.worktree))
                    && activities < ACTIVITY_LIMIT
                {
                    activities += 1;
                    on_activity(clip(&text, TEXT_LIMIT));
                }
            }
            "item/completed" => {
                let item = params.get("item");
                if item.and_then(|i| i.get("type")).and_then(Value::as_str) == Some("agentMessage")
                    && let Some(text) = item.and_then(|i| i.get("text")).and_then(Value::as_str)
                {
                    out.last_message = text.to_string();
                }
            }
            "thread/tokenUsage/updated" => {
                let last = params.pointer("/tokenUsage/last");
                let read = |key: &str| {
                    last.and_then(|u| u.get(key))
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                };
                out.usage.input_tokens += read("inputTokens");
                out.usage.cached_input_tokens += read("cachedInputTokens");
                out.usage.output_tokens += read("outputTokens");
            }
            "turn/completed" => {
                let status = params
                    .pointer("/turn/status")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if status != "completed" && interrupted_at.is_none() {
                    out.status = "error";
                    out.errors.push(
                        params
                            .pointer("/turn/error/message")
                            .and_then(Value::as_str)
                            .map_or_else(|| format!("el turno acabó con {status}"), str::to_string),
                    );
                }
                break;
            }
            _ => {}
        }
    }
    if interrupted_at.is_some() {
        out.status = "timeout";
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

/// The activity line of an `item/started`, if its kind is shown.
fn activity(item: &Value, cwd: &Path) -> Option<String> {
    match item.get("type").and_then(Value::as_str)? {
        "commandExecution" => {
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
        "fileChange" => {
            let root = absolute(cwd);
            let paths: Vec<String> = item
                .get("changes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|change| change.get("path").and_then(Value::as_str))
                .map(|path| {
                    Path::new(path)
                        .strip_prefix(&root)
                        .map(|rest| rest.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| path.to_string())
                })
                .collect();
            Some(format!("edita {}", paths.join(", ")))
        }
        "mcpToolCall" => Some("mcp_tool_call".into()),
        "webSearch" => Some("web_search".into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
