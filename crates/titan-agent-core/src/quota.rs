//! Remaining quota of each CLI's subscription, from official sources only
//! (docs/Tareas/Investigar la cuota restante de cada CLI.md):
//!
//! - Codex: `account/rateLimits/read` of `codex app-server`.
//! - Claude Code: the `rate_limits` object of the statusline input, which the
//!   user may copy to a file with their statusline (nothing else exposes it).
//!
//! Nothing here reads credentials or calls an API (ADR-0001).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session::AgentCli;

/// Remaining quota under which the monitor shows it in amber.
pub const LOW_PERCENT: f64 = 30.0;

/// One usage window of a subscription.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quota {
    pub agent: AgentCli,
    /// Limit name (`codex`, or a per-model limit).
    pub limit: String,
    /// Model the limit applies to, when the CLI says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Length of the window in minutes (300 = 5 h, 10080 = a week).
    pub window_mins: u32,
    /// Percentage used, 0 to 100.
    pub used_percent: f64,
    /// Unix time in seconds when the window resets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
}

impl Quota {
    pub fn remaining_percent(&self) -> f64 {
        (100.0 - self.used_percent).clamp(0.0, 100.0)
    }

    pub fn is_low(&self) -> bool {
        self.remaining_percent() <= LOW_PERCENT
    }

    /// `5 h`, `semanal` or the minutes.
    pub fn window_label(&self) -> String {
        match self.window_mins {
            300 => "5 h".into(),
            10080 => "semanal".into(),
            m if m % 60 == 0 => format!("{} h", m / 60),
            m => format!("{m} min"),
        }
    }
}

/// Parses the result of Codex's `account/rateLimits/read`. Unknown fields
/// are ignored and a missing window is skipped.
pub fn parse_codex_rate_limits(result: &Value) -> Vec<Quota> {
    let mut quotas = Vec::new();
    let mut push = |limit: &Value, fallback_id: &str| {
        let id = limit
            .get("limitId")
            .and_then(Value::as_str)
            .unwrap_or(fallback_id)
            .to_string();
        let model = limit
            .get("normalModelSlug")
            .and_then(Value::as_str)
            .map(str::to_string);
        for key in ["primary", "secondary"] {
            let Some(window) = limit.get(key).filter(|w| w.is_object()) else {
                continue;
            };
            let (Some(used), Some(mins)) = (
                window.get("usedPercent").and_then(Value::as_f64),
                window.get("windowDurationMins").and_then(Value::as_u64),
            ) else {
                continue;
            };
            let quota = Quota {
                agent: AgentCli::Codex,
                limit: id.clone(),
                model: model.clone(),
                window_mins: mins as u32,
                used_percent: used,
                resets_at: window.get("resetsAt").and_then(Value::as_i64),
            };
            if !quotas
                .iter()
                .any(|q: &Quota| q.limit == quota.limit && q.window_mins == quota.window_mins)
            {
                quotas.push(quota);
            }
        }
    };
    if let Some(by_id) = result.get("rateLimitsByLimitId").and_then(Value::as_object) {
        for (id, limit) in by_id {
            push(limit, id);
        }
    }
    if let Some(limit) = result.get("rateLimits") {
        push(limit, "codex");
    }
    quotas
}

/// Parses the `rate_limits` object of Claude Code's statusline input (or the
/// whole statusline input). Windows already reset at `now_s` are dropped.
pub fn parse_claude_rate_limits(value: &Value, now_s: i64) -> Vec<Quota> {
    let limits = value.get("rate_limits").unwrap_or(value);
    let mut quotas = Vec::new();
    for (key, mins) in [("five_hour", 300), ("seven_day", 10080)] {
        let Some(window) = limits.get(key) else {
            continue;
        };
        let Some(used) = window.get("used_percentage").and_then(Value::as_f64) else {
            continue;
        };
        let resets_at = window.get("resets_at").and_then(Value::as_i64);
        if resets_at.is_some_and(|at| at <= now_s) {
            continue;
        }
        quotas.push(Quota {
            agent: AgentCli::ClaudeCode,
            limit: "claude".into(),
            model: None,
            window_mins: mins,
            used_percent: used,
            resets_at,
        });
    }
    quotas
}

/// The quota to show for an agent of `cli` running `model`: the shortest
/// window of a limit for that model, else of the CLI's general limit.
pub fn quota_for<'a>(quotas: &'a [Quota], cli: AgentCli, model: Option<&str>) -> Option<&'a Quota> {
    let of_cli = quotas.iter().filter(|q| q.agent == cli);
    let for_model: Vec<&Quota> = of_cli
        .clone()
        .filter(|q| model.is_some() && q.model.as_deref() == model)
        .collect();
    let pool: Vec<&Quota> = if for_model.is_empty() {
        of_cli.filter(|q| q.model.is_none()).collect()
    } else {
        for_model
    };
    pool.into_iter().min_by_key(|q| q.window_mins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_limits_by_id_and_general() {
        // Shape of codex-cli 0.169.0, values invented.
        let result = json!({
            "rateLimits": {"limitId": "codex", "primary": {"usedPercent": 3, "windowDurationMins": 300, "resetsAt": 10},
                           "secondary": {"usedPercent": 19, "windowDurationMins": 10080, "resetsAt": 20}},
            "rateLimitsByLimitId": {
                "codex": {"limitId": "codex", "primary": {"usedPercent": 3, "windowDurationMins": 300, "resetsAt": 10}},
                "sol": {"limitId": "sol", "normalModelSlug": "gpt-6.1-sol",
                        "primary": {"usedPercent": 80, "windowDurationMins": 300}, "secondary": null}
            },
            "newField": true
        });
        let quotas = parse_codex_rate_limits(&result);
        assert_eq!(quotas.len(), 3, "{quotas:?}");
        let sol = quota_for(&quotas, AgentCli::Codex, Some("gpt-6.1-sol")).unwrap();
        assert_eq!(sol.limit, "sol");
        assert!(sol.is_low());
        let luna = quota_for(&quotas, AgentCli::Codex, Some("gpt-6-luna")).unwrap();
        assert_eq!((luna.limit.as_str(), luna.window_mins), ("codex", 300));
        assert_eq!(luna.remaining_percent(), 97.0);
        assert!(quota_for(&quotas, AgentCli::ClaudeCode, None).is_none());
    }

    #[test]
    fn claude_statusline_windows() {
        let input = json!({"model": {}, "rate_limits": {
            "five_hour": {"used_percentage": 42.5, "resets_at": 2000},
            "seven_day": {"used_percentage": 10.0, "resets_at": 500}
        }});
        let quotas = parse_claude_rate_limits(&input, 1000);
        assert_eq!(quotas.len(), 1, "the weekly window already reset");
        assert_eq!(quotas[0].window_label(), "5 h");
        assert_eq!(quotas[0].remaining_percent(), 57.5);
        assert!(parse_claude_rate_limits(&json!({}), 0).is_empty());
    }
}
