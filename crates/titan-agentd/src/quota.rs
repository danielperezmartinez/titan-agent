//! Remaining quota of each CLI, from official sources only
//! (docs/Tareas/Investigar la cuota restante de cada CLI.md):
//!
//! - Codex: `account/rateLimits/read` of the shared `codex app-server`, with
//!   `excludeResetCreditDetails` (the option meant for periodic reads). No
//!   other `account/` method is called.
//! - Claude Code: the `rate_limits` object of the statusline input, if the
//!   user's statusline copies it to `<state>/titan-agent/claude-rate-limits.json`
//!   (`$XDG_STATE_HOME`, or `~/.local/state`). The daemon only reads it.

use std::path::PathBuf;

use serde_json::{Value, json};
use titan_agent_core::quota::{Quota, parse_claude_rate_limits, parse_codex_rate_limits};

use crate::codex::{CodexClient, socket_path};

/// Bridge file the user's statusline may write.
pub fn claude_bridge(home: Option<&PathBuf>) -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".local/state")))?;
    Some(state.join("titan-agent").join("claude-rate-limits.json"))
}

/// Quotas of both CLIs; a CLI that cannot be read is left out.
pub fn read(codex: &str, home: Option<&PathBuf>) -> Vec<Quota> {
    let mut quotas = read_codex(codex).unwrap_or_default();
    if let Some(path) = claude_bridge(home)
        && let Ok(text) = std::fs::read_to_string(path)
        && let Ok(value) = serde_json::from_str::<Value>(&text)
    {
        let now_s = titan_agent_core::time::now_ms() / 1000;
        quotas.extend(parse_claude_rate_limits(&value, now_s));
    }
    quotas
}

fn read_codex(codex: &str) -> Result<Vec<Quota>, String> {
    let path = socket_path(codex)?;
    let mut client = CodexClient::connect(&path)?;
    let result = client.request(
        "account/rateLimits/read",
        json!({"excludeResetCreditDetails": true}),
    )?;
    Ok(parse_codex_rate_limits(&result))
}
