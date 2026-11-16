//! Project monitor (docs/Decisiones visuales/Monitor del proyecto en tres
//! vistas.md): bar with the task, round and the three view buttons, and the
//! pieces the views share. Each view lives in its own submodule: `graph`,
//! `lanes` and `desk`. What they draw comes from
//! `titan_agent_core::monitor::Board`.

use gpui::{ClickEvent, Context, Div, FontWeight, Hsla, SharedString, div, prelude::*, px};
use titan_agent_core::monitor::{Agent, Board, board, latest_team};
use titan_agent_core::session::{AgentCli, Session, SessionState};
use titan_agent_core::team::USER;

use crate::theme::{self, color, hairline, hsla};

use super::widgets::placeholder;
use super::{MainView, Selection};

mod desk;
mod graph;
mod lanes;

/// View shown in the monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MonitorTab {
    Graph,
    Lanes,
    Desk,
}

impl MonitorTab {
    const ALL: [(MonitorTab, &'static str); 3] = [
        (MonitorTab::Graph, "grafo"),
        (MonitorTab::Lanes, "carriles"),
        (MonitorTab::Desk, "mesa de control"),
    ];

    /// `TITAN_AGENT_MONITOR` (screenshots): `grafo`, `carriles` or `mesa`.
    pub(super) fn from_env() -> MonitorTab {
        match std::env::var("TITAN_AGENT_MONITOR").as_deref() {
            Ok("carriles") => MonitorTab::Lanes,
            Ok("mesa") => MonitorTab::Desk,
            _ => MonitorTab::Graph,
        }
    }
}

/// Time windows of the lanes view, in seconds.
pub(super) const LANE_WINDOWS: [i64; 3] = [36, 300, 1800];

/// Spinner of a working agent: `| / - \`, one step every 300 ms.
pub(super) fn spinner(now_ms: i64) -> &'static str {
    ["|", "/", "-", "\\"][(now_ms / 300).rem_euclid(4) as usize]
}

/// 0 to 1 and back over `period_ms`, for things that pulse.
pub(super) fn pulse(now_ms: i64, period_ms: i64) -> f32 {
    let phase = (now_ms.rem_euclid(period_ms)) as f32 / period_ms as f32;
    0.5 - 0.5 * (phase * std::f32::consts::TAU).cos()
}

pub(super) fn agent_hex(cli: Option<AgentCli>) -> u32 {
    cli.map_or(theme::INK, theme::agent_color)
}

pub(super) fn with_alpha(hex: u32, alpha: f32) -> Hsla {
    let mut c = hsla(hex);
    c.a = alpha;
    c
}

/// How an agent looks right now: marker, its colour, action colour, border.
pub(super) struct Look {
    pub marker: String,
    pub marker_hex: u32,
    pub action: String,
    pub action_hex: u32,
    pub border: Hsla,
    pub dim: bool,
}

/// Look of an agent, given the message travelling now.
pub(super) fn look(board: &Board, agent: &Agent, now_ms: i64) -> Look {
    let live = board.live_message(now_ms);
    let mut l = Look {
        marker: "[+]".into(),
        marker_hex: theme::MUTE,
        action: agent.action.clone(),
        action_hex: theme::MUTE,
        border: hairline().into(),
        dim: true,
    };
    match agent.state {
        SessionState::Working => {
            l.marker = format!("[>] {}", spinner(now_ms));
            l.marker_hex = theme::SUCCESS;
            l.action_hex = theme::BODY;
            l.border = hsla(theme::SUCCESS);
            l.dim = false;
        }
        SessionState::Finished { .. } => l.marker = "[x]".into(),
        _ => {}
    }
    if let Some(m) = live {
        if m.from == agent.id && m.to != USER {
            l.marker = "[>]".into();
            l.marker_hex = agent_hex(agent.cli);
            l.action = format!("envía a {}: {}", board.label(&m.to), m.text);
            l.action_hex = theme::BODY;
            l.border = hsla(agent_hex(agent.cli));
            l.dim = false;
        }
        if m.to == agent.id {
            l.action = if agent.cli.is_none() {
                "te piden una decisión".into()
            } else {
                format!("recibe de {}: {}", board.label(&m.from), m.text)
            };
            l.action_hex = theme::BODY;
            l.border = hsla(agent_hex(m.sender));
            l.dim = false;
        }
    }
    if agent.state == SessionState::NeedsYou {
        l.marker = "[?]".into();
        l.marker_hex = theme::WARNING;
        if agent.cli.is_some() {
            l.action = "te necesita".into();
        }
        l.action_hex = 0xffb340;
        l.border = with_alpha(theme::WARNING, 0.45 + 0.55 * pulse(now_ms, 1400));
        l.dim = false;
    }
    l
}

/// «N s en este estado» / «N min en este estado».
pub(super) fn in_state(since_ms: Option<i64>, now_ms: i64) -> String {
    let Some(since) = since_ms else {
        return String::new();
    };
    let seconds = ((now_ms - since) / 1000).max(0);
    if seconds < 60 {
        format!("{seconds} s en este estado")
    } else if seconds < 3600 {
        format!("{} min en este estado", seconds / 60)
    } else {
        format!("{} h en este estado", seconds / 3600)
    }
}

impl MainView {
    /// The board of the project's newest open task, if it has one.
    pub(super) fn board_for(&self, root: &str) -> Option<Board> {
        let (task, run) = latest_team(&self.teams, root)?;
        Some(board(&task, &run, &self.sessions, self.now_ms))
    }

    /// Whether the centre shows a live monitor (it then redraws often).
    pub(super) fn monitor_animating(&self) -> bool {
        match &self.selection {
            Selection::Project(root) => self.teams.iter().any(|t| &t.project_root == root),
            _ => false,
        }
    }

    /// Opens the session of an agent clicked in the monitor.
    pub(super) fn open_agent(
        &mut self,
        session: Option<String>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        match session {
            Some(key) => self.select_session(key, Some(window), cx),
            None => {
                self.notice = Some(("ese agente no tiene una sesión a la vista".into(), true));
                cx.notify();
            }
        }
    }

    pub(super) fn monitor_view(&self, root: &str, cx: &mut Context<Self>) -> Div {
        let sessions: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|s| s.project_root == root)
            .collect();
        let name = sessions
            .first()
            .map(|s| s.project.clone())
            .unwrap_or_else(|| titan_agent_core::project::project_name(root));
        let board = self.board_for(root);
        let now = self.now_ms;
        let subtitle = match &board {
            Some(b) => {
                let mut text = format!("encargo {}", b.task);
                if b.max_rounds > 0 {
                    text.push_str(&format!(" · ronda {} de {}", b.round.max(1), b.max_rounds));
                }
                text.push_str(&format!(" · {} agentes", b.agents.len() - 1));
                text
            }
            None => format!(
                "{} sesión(es), ninguna en equipo",
                sessions.iter().filter(|s| s.parent.is_none()).count()
            ),
        };
        let tabs = MonitorTab::ALL.map(|(tab, label)| {
            let selected = self.monitor_tab == tab;
            div()
                .id(SharedString::from(format!("monitor-tab-{label}")))
                .flex()
                .items_center()
                .h(px(30.))
                .px(px(12.))
                .border_1()
                .rounded(px(4.))
                .border_color(if selected {
                    color(theme::ACCENT).into()
                } else {
                    Hsla::from(hairline())
                })
                .when(selected, |d| d.bg(color(theme::SURFACE_ELEVATED)))
                .text_color(color(if selected { theme::INK } else { theme::MUTE }))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.monitor_tab = tab;
                    cx.notify();
                }))
                .child(label)
        });
        let bar = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(16.))
            .h(px(40.))
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
            .child(
                div()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(subtitle),
            )
            .when(board.is_some(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_none()
                        .gap(px(6.))
                        .text_color(color(theme::SUCCESS))
                        .child(
                            div()
                                .opacity(0.25 + 0.75 * pulse(now + 800, 1600))
                                .child("[*]"),
                        )
                        .child("en vivo"),
                )
            })
            .child(div().flex_grow_1())
            .child(div().flex().flex_none().gap(px(4.)).children(tabs));
        let body = match board {
            Some(board) => match self.monitor_tab {
                MonitorTab::Graph => self.graph_view(&board, cx).into_any_element(),
                MonitorTab::Lanes => self.lanes_view(&board, cx).into_any_element(),
                MonitorTab::Desk => self.desk_view(&board, cx).into_any_element(),
            },
            None => {
                let loose: Vec<String> = sessions
                    .iter()
                    .filter(|s| s.parent.is_none())
                    .map(|s| s.name.clone())
                    .collect();
                let mut list = loose.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
                if loose.len() > 6 {
                    list.push_str(&format!(" y {} más", loose.len() - 6));
                }
                placeholder(
                    &format!("Sin equipo trabajando en {name}"),
                    vec![
                        "El monitor se anima cuando las sesiones del proyecto se encargan trabajo entre ellas."
                            .into(),
                        format!(
                            "Sesiones sueltas: {}",
                            if list.is_empty() { "ninguna".into() } else { list }
                        ),
                    ],
                )
                .into_any_element()
            }
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(bar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_grow_1()
                    .min_h(px(0.))
                    .bg(color(theme::TERMINAL_BG))
                    .child(body),
            )
            .when_some(self.notice.clone(), |d, (text, error)| {
                d.child(
                    div()
                        .px(px(16.))
                        .py(px(6.))
                        .bg(color(theme::TERMINAL_BG))
                        .text_size(px(12.))
                        .text_color(color(if error { theme::DANGER } else { theme::MUTE }))
                        .child(text),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_and_pulse_cycle() {
        assert_eq!(
            [0, 300, 600, 900, 1200].map(spinner),
            ["|", "/", "-", "\\", "|"]
        );
        assert!(pulse(0, 1000) < 0.01);
        assert!((pulse(500, 1000) - 1.0).abs() < 0.01);
        assert_eq!(in_state(Some(0), 61_000), "1 min en este estado");
        assert_eq!(in_state(None, 5), "");
    }
}
