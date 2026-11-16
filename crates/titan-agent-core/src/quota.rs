//! Remaining quota of a CLI subscription, read only through official ways
//! (docs/Tareas/Investigar la cuota restante de cada CLI.md).
//!
//! Codex reports it with `account/rateLimits/read` of `codex app-server`;
//! Claude Code has no official way usable by the daemon, so it never appears.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session::AgentCli;

/// Below this remaining share a quota is shown as low.
pub const LOW_REMAINING_PERCENT: f64 = 30.0;

/// One rate-limit window of a CLI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quota {
    pub agent: AgentCli,
    /// Limit id reported by the CLI, for example `codex`.
    pub limit: String,
    /// Window length: 300 is five hours, 10080 one week.
    pub window_mins: u32,
    /// Used share, 0 to 100 (not the remaining one).
    pub used_percent: f64,
    #[serde(default)]
    pub resets_at_ms: Option<i64>,
}

impl Quota {
    /// Remaining share, 0 to 100.
    pub fn remaining_percent(&self) -> f64 {
        (100.0 - self.used_percent).clamp(0.0, 100.0)
    }

    pub fn is_low(&self) -> bool {
        self.remaining_percent() <= LOW_REMAINING_PERCENT
    }

    /// Short window name for the interface: `5 h`, `semana`, or minutes.
    pub fn window_label(&self) -> String {
        match self.window_mins {
            10080 => "semana".into(),
            m if m % 60 == 0 => format!("{} h", m / 60),
            m => format!("{m} min"),
        }
    }
}

/// Reads the result of `account/rateLimits/read`. Tolerant: unknown fields
/// are ignored and a malformed entry only drops itself. Nothing about the
/// account (ids, credits) is kept.
pub fn parse_codex_rate_limits(result: &Value) -> Vec<Quota> {
    let entries: Vec<(String, &Value)> = match result
        .get("rateLimitsByLimitId")
        .and_then(Value::as_object)
    {
        Some(map) => map.iter().map(|(key, value)| (key.clone(), value)).collect(),
        None => result
            .get("rateLimits")
            .map(|value| (String::new(), value))
            .into_iter()
            .collect(),
    };
    let mut quotas = Vec::new();
    for (key, entry) in entries {
        if !entry.is_object() {
            continue;
        }
        let limit = entry
            .get("limitId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(key);
        for window in ["primary", "secondary"] {
            let Some(window) = entry.get(window) else {
                continue;
            };
            let used = window.get("usedPercent").and_then(Value::as_f64);
            let minutes = window
                .get("windowDurationMins")
                .and_then(Value::as_u64)
                .and_then(|m| u32::try_from(m).ok());
            let (Some(used), Some(minutes)) = (used, minutes) else {
                continue;
            };
            quotas.push(Quota {
                agent: AgentCli::Codex,
                limit: limit.clone(),
                window_mins: minutes,
                used_percent: used,
                resets_at_ms: window
                    .get("resetsAt")
                    .and_then(Value::as_i64)
                    .map(|seconds| seconds * 1000),
            });
        }
    }
    quotas
}

/// The quota to show for an agent: its shortest window (the one that runs
/// out first in practice).
pub fn shortest_window(quotas: &[Quota], agent: AgentCli) -> Option<&Quota> {
    quotas
        .iter()
        .filter(|q| q.agent == agent)
        .min_by_key(|q| q.window_mins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn without_the_map_the_single_entry_is_read() {
        let result = json!({"rateLimits": {"limitId": "codex",
            "primary": {"usedPercent": 72.5, "windowDurationMins": 300}}});
        let quotas = parse_codex_rate_limits(&result);
        assert_eq!(quotas.len(), 1);
        assert!(quotas[0].is_low());
        assert_eq!(quotas[0].window_label(), "5 h");
        assert_eq!(quotas[0].resets_at_ms, None);
    }

    #[test]
    fn the_shortest_window_of_the_agent_is_shown() {
        let result = json!({"rateLimitsByLimitId": {"codex": {
            "primary": {"usedPercent": 3, "windowDurationMins": 300, "resetsAt": 10},
            "secondary": {"usedPercent": 19, "windowDurationMins": 10080}}, "odd": 4}});
        let quotas = parse_codex_rate_limits(&result);
        assert_eq!(quotas.len(), 2);
        assert_eq!(quotas[0].resets_at_ms, Some(10_000));
        let shown = shortest_window(&quotas, AgentCli::Codex).unwrap();
        assert_eq!(shown.window_mins, 300);
        assert_eq!(quotas[1].window_label(), "semana");
        assert!(shortest_window(&quotas, AgentCli::ClaudeCode).is_none());
    }
}
