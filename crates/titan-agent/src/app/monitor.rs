//! Project monitor (docs/Decisiones visuales/Monitor del proyecto en tres
//! vistas.md): its bar with the three view buttons and, below, the view, or
//! the project's loose sessions when no team works in it.
//!
//! What every view shows comes from `titan_agent_core::monitor::Monitor`;
//! the views live in `monitor_graph`, `monitor_lanes` and `monitor_desk`.

use gpui::{ClickEvent, Context, Div, FontWeight, Rgba, SharedString, div, prelude::*, px};
use titan_agent_core::monitor::{AgentStatus, IDLE_MS, Monitor, MonitorAgent, YOU};
use titan_agent_core::session::{AgentCli, Session};
use titan_agent_core::team::run::TeamRun;

use crate::theme::{self, color, hairline};

use super::widgets::placeholder;
use super::{MainView, Selection};

/// The three views, in the order of their buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MonitorTab {
    Graph,
    Lanes,
    Desk,
}

impl MonitorTab {
    const ALL: [MonitorTab; 3] = [MonitorTab::Graph, MonitorTab::Lanes, MonitorTab::Desk];

    fn label(self) -> &'static str {
        match self {
            Self::Graph => "grafo",
            Self::Lanes => "carriles",
            Self::Desk => "mesa de control",
        }
    }

    /// `TITAN_AGENT_MONITOR_VIEW` (`grafo`, `carriles`, `mesa`), for
    /// screenshots; the graph otherwise.
    pub(super) fn from_env() -> MonitorTab {
        match std::env::var("TITAN_AGENT_MONITOR_VIEW").as_deref() {
            Ok("carriles") => Self::Lanes,
            Ok("mesa") => Self::Desk,
            _ => Self::Graph,
        }
    }
}

/// Time windows of the lanes view; the first is the one of the decision.
pub(super) const LANE_WINDOWS: [(i64, &str); 3] =
    [(36_000, "36 s"), (300_000, "5 min"), (1_800_000, "30 min")];

impl MainView {
    /// The task a team is working on in a project, the newest first.
    pub(super) fn active_team(&self, root: &str) -> Option<&TeamRun> {
        self.teams
            .iter()
            .filter(|run| run.project_root == root && run.is_active(self.now_ms, IDLE_MS))
            .max_by_key(|run| run.updated_ms)
    }

    pub(super) fn project_view(&mut self, root: &str, cx: &mut Context<Self>) -> Div {
        let sessions: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|s| s.project_root == root)
            .collect();
        let name = sessions
            .first()
            .map(|s| s.project.clone())
            .unwrap_or_else(|| titan_agent_core::project::project_name(root));
        let monitor = self.active_team(root).map(|run| {
            Monitor::build(
                run,
                &self.sessions,
                &self.state_log,
                &self.quotas,
                self.now_ms,
            )
        });
        let subtitle = match &monitor {
            Some(m) => m.subtitle(),
            None => format!(
                "{} sesión(es), ninguna en equipo",
                sessions.iter().filter(|s| s.parent.is_none()).count()
            ),
        };
        let tabs = MonitorTab::ALL.iter().map(|&tab| {
            let selected = self.monitor_tab == tab;
            div()
                .id(SharedString::from(format!("tab-{}", tab.label())))
                .flex()
                .items_center()
                .min_h(px(30.))
                .px(px(12.))
                .border_1()
                .border_color(if selected {
                    color(theme::ACCENT)
                } else {
                    hairline()
                })
                .rounded(px(4.))
                .when(selected, |d| d.bg(color(theme::SURFACE_ELEVATED)))
                .text_color(color(if selected { theme::INK } else { theme::MUTE }))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.monitor_tab = tab;
                    cx.notify();
                }))
                .child(tab.label())
        });
        let live = monitor.is_some();
        let bar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.))
            .px(px(16.))
            .py(px(4.))
            .min_h(px(40.))
            .flex_none()
            .border_b_1()
            .border_color(hairline())
            .text_size(px(12.))
            .text_color(color(theme::MUTE))
            .child(
                div()
                    .text_color(color(theme::INK))
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(13.))
                    .child(format!("[#] {name}")),
            )
            .child(subtitle)
            .when(live, |d| {
                d.child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .text_color(color(theme::SUCCESS))
                        .child(div().opacity(pulse(self.now_ms, 1600, 0.25)).child("[*]"))
                        .child("en vivo"),
                )
            })
            .child(div().flex().gap(px(4.)).ml_auto().children(tabs));
        let body = match monitor {
            Some(monitor) => {
                let view = match self.monitor_tab {
                    MonitorTab::Graph => self.graph_view(&monitor, cx).into_any_element(),
                    MonitorTab::Lanes => self.lanes_view(&monitor, cx).into_any_element(),
                    MonitorTab::Desk => self.desk_view(&monitor, cx).into_any_element(),
                };
                div()
                    .id("monitor-body")
                    .flex()
                    .flex_col()
                    .flex_grow_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .bg(color(theme::TERMINAL_BG))
                    .child(view)
                    .into_any_element()
            }
            None => self.no_team(&name, &sessions).into_any_element(),
        };
        div().flex().flex_col().size_full().child(bar).child(body)
    }

    fn no_team(&self, name: &str, sessions: &[&Session]) -> Div {
        // Top-level sessions by name; threads launched by another one are
        // only counted, so a long team run does not flood the panel.
        let top: Vec<String> = sessions
            .iter()
            .filter(|s| s.parent.is_none())
            .map(|s| s.name.clone())
            .collect();
        let shown = top.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
        let mut list = format!(
            "Sesiones sueltas: {}",
            if shown.is_empty() { "ninguna" } else { &shown }
        );
        let more = top.len().saturating_sub(6);
        if more > 0 {
            list.push_str(&format!(" y {more} más"));
        }
        let mut lines = vec![
            "El monitor se anima cuando las sesiones del proyecto se encargan trabajo entre ellas."
                .to_string(),
            list,
        ];
        let nested = sessions.len() - top.len();
        if nested > 0 {
            lines.push(format!(
                "{nested} hilo(s) lanzados por otras sesiones en los últimos 20 min."
            ));
        }
        placeholder(&format!("Sin equipo trabajando en {name}"), lines)
    }

    /// Opens the session of an agent of the monitor, if it has one.
    pub(super) fn open_agent(&mut self, session: Option<String>, cx: &mut Context<Self>) {
        if let Some(key) = session
            && self.sessions.iter().any(|s| s.key == key)
        {
            self.select_session(key, None, cx);
        }
    }

    /// Faster frames while a monitor with a team is on screen.
    pub(super) fn animating(&self) -> bool {
        matches!(&self.selection, Selection::Project(root) if self.active_team(root).is_some())
    }
}

/// Colour of an agent's identity: Claude violet, Codex cyan, the user ink.
pub(super) fn agent_tint(agent: &MonitorAgent) -> u32 {
    match agent.agent {
        Some(cli) => theme::agent_color(cli),
        None => theme::INK,
    }
}

/// `cl`, `cx` or nothing for the user.
pub(super) fn agent_tag(agent: &MonitorAgent) -> &'static str {
    match agent.agent {
        Some(AgentCli::ClaudeCode) => "cl",
        Some(AgentCli::Codex) => "cx",
        None => "",
    }
}

/// The marker of a status, with the ASCII spinner while working.
pub(super) fn status_mark(status: AgentStatus, now_ms: i64) -> (String, u32) {
    match status {
        AgentStatus::Working => (format!("[>] {}", spinner(now_ms)), theme::SUCCESS),
        AgentStatus::NeedsYou => ("[?]".into(), theme::WARNING),
        AgentStatus::Waiting | AgentStatus::Done => (status.marker().into(), theme::MUTE),
    }
}

pub(super) fn spinner(now_ms: i64) -> char {
    ['|', '/', '-', '\\'][(now_ms / 300).rem_euclid(4) as usize]
}

/// Opacity of something that beats: 1, down to `low` and back, every
/// `period_ms`.
pub(super) fn pulse(now_ms: i64, period_ms: i64, low: f32) -> f32 {
    let phase = (now_ms.rem_euclid(period_ms)) as f32 / period_ms as f32;
    let wave = (phase * std::f32::consts::TAU).cos() * 0.5 + 0.5;
    low + (1.0 - low) * wave
}

/// The amber of «te necesita», beating (the border of a node or card).
pub(super) fn needs_border(now_ms: i64) -> Rgba {
    let mut warning = color(theme::WARNING);
    warning.a = pulse(now_ms, 1400, 0.45);
    warning
}

/// Name of an agent in the views' sentences (`tú`, `opus`, `sol·tests`).
pub(super) fn agent_line(monitor: &Monitor, index: usize) -> String {
    if index == YOU {
        "tú".into()
    } else {
        monitor.agents[index].name.clone()
    }
}

/// `n s`, `n min` or `n h`.
pub(super) fn duration(ms: i64) -> String {
    let s = (ms.max(0)) / 1000;
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} min", s / 60),
        _ => format!("{} h {} min", s / 3600, (s % 3600) / 60),
    }
}

/// `115k`, `1,2M`.
pub(super) fn tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{}k", n / 1000),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0).replace('.', ","),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_formats() {
        assert_eq!(duration(12_400), "12 s");
        assert_eq!(duration(125_000), "2 min");
        assert_eq!(duration(3_660_000), "1 h 1 min");
        assert_eq!(tokens(115_200), "115k");
        assert_eq!(tokens(1_370_000), "1,4M");
        assert_eq!(spinner(0), '|');
        assert_eq!(spinner(900), '\\');
        assert!((pulse(0, 1000, 0.25) - 1.0).abs() < 1e-6);
        assert!((pulse(500, 1000, 0.25) - 0.25).abs() < 1e-6);
    }
}
