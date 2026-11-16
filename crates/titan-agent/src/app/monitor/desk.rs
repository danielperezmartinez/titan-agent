//! Control desk view of the monitor (preview `MonitorC`, in the style of
//! `htop`): a strip with who is talking, working or needs you, with an
//! animated ASCII line and the review round; then one card per agent with
//! its state, action, recent activity, turns and tokens, and its quota.

use gpui::{
    ClickEvent, Context, Div, FontWeight, Hsla, SharedString, Stateful, div, prelude::*, px,
};
use titan_agent_core::monitor::{Agent, Bar, Board};
use titan_agent_core::quota::shortest_window;
use titan_agent_core::session::{AgentCli, SessionState};
use titan_agent_core::team::USER;

use crate::theme::{self, color, hairline, hsla};

use super::super::MainView;
use super::{agent_hex, in_state, look, spinner, with_alpha};

/// Bars of the activity histogram and the time each one covers.
const BARS: usize = 24;
const BAR_MS: i64 = 15_000;
/// Length of the ASCII line of the strip and one trip of its `*`.
const LINE: usize = 24;
const LINE_TRIP_MS: i64 = 2400;

fn tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M tok", n as f64 / 1_000_000.)
    } else if n >= 1000 {
        format!("{}k tok", n / 1000)
    } else {
        format!("{n} tok")
    }
}

impl MainView {
    pub(super) fn desk_view(&self, board: &Board, cx: &mut Context<Self>) -> Stateful<Div> {
        let cards: Vec<&Agent> = board.agents.iter().filter(|a| a.id != USER).collect();
        // Director across the whole width, the rest two by two (an odd
        // last one also across).
        let mut rows: Vec<Vec<&Agent>> = Vec::new();
        let mut pending: Vec<&Agent> = Vec::new();
        for agent in cards {
            if agent.is_director {
                rows.push(vec![agent]);
            } else {
                pending.push(agent);
                if pending.len() == 2 {
                    rows.push(std::mem::take(&mut pending));
                }
            }
        }
        if !pending.is_empty() {
            rows.push(pending);
        }
        div()
            .id("desk")
            .flex()
            .flex_col()
            .size_full()
            .gap(px(16.))
            .p(px(16.))
            .overflow_y_scroll()
            .child(self.desk_strip(board))
            .children(rows.into_iter().map(|row| {
                div().flex().gap(px(16.)).children(
                    row.into_iter()
                        .map(|agent| self.desk_card(board, agent, cx)),
                )
            }))
    }

    fn desk_strip(&self, board: &Board) -> Div {
        let now = self.now_ms;
        let live = board.live_message(now);
        let (title, from, to, line, line_hex, from_hex, to_hex, label, border): (
            &str,
            String,
            String,
            String,
            u32,
            u32,
            u32,
            String,
            Hsla,
        ) = match live {
            Some(m) => {
                let pos = (((now - m.at_ms).rem_euclid(LINE_TRIP_MS)) as usize * LINE
                    / LINE_TRIP_MS as usize)
                    .min(LINE - 1);
                let mut ascii: String = (0..LINE)
                    .map(|i| if i == pos { '*' } else { '-' })
                    .collect();
                ascii.push('>');
                let needs = m.to == USER;
                let sender = agent_hex(m.sender);
                let receiver = agent_hex(board.agent(&m.to).and_then(|a| a.cli));
                (
                    if needs { "TE NECESITAN" } else { "HABLANDO" },
                    board.label(&m.from),
                    board.label(&m.to),
                    ascii,
                    sender,
                    sender,
                    receiver,
                    m.text.clone(),
                    if needs {
                        with_alpha(theme::WARNING, 0.45 + 0.55 * super::pulse(now, 1400))
                    } else {
                        hsla(sender)
                    },
                )
            }
            None => {
                let working = board
                    .agents
                    .iter()
                    .find(|a| a.state == SessionState::Working && !a.is_director)
                    .or_else(|| {
                        board
                            .agents
                            .iter()
                            .find(|a| a.state == SessionState::Working)
                    });
                match working {
                    Some(a) => (
                        "TRABAJANDO",
                        a.title.clone(),
                        String::new(),
                        spinner(now).to_string(),
                        theme::SUCCESS,
                        agent_hex(a.cli),
                        theme::MUTE,
                        a.action.clone(),
                        hairline().into(),
                    ),
                    None => (
                        "EN ESPERA",
                        String::new(),
                        String::new(),
                        String::new(),
                        theme::MUTE,
                        theme::MUTE,
                        theme::MUTE,
                        board.headline(now),
                        hairline().into(),
                    ),
                }
            }
        };
        let round = board.round.max(1);
        let segments = (1..=board.max_rounds).map(|r| {
            let tone = if r < round {
                theme::WARNING
            } else if r == round {
                theme::SUCCESS
            } else {
                theme::SURFACE_ELEVATED
            };
            div().w(px(18.)).h(px(8.)).bg(color(tone))
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.))
            .p(px(16.))
            .bg(color(theme::CANVAS))
            .border_1()
            .border_color(border)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(color(theme::MUTE))
                    .child(title),
            )
            .when(!from.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .gap(px(16.))
                        .text_size(px(18.))
                        .child(
                            div()
                                .text_color(color(from_hex))
                                .font_weight(FontWeight::BOLD)
                                .child(from),
                        )
                        .child(div().text_color(color(line_hex)).child(line))
                        .when(!to.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_color(color(to_hex))
                                    .font_weight(FontWeight::BOLD)
                                    .child(to),
                            )
                        }),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(px(14.))
                    .text_color(color(theme::BODY))
                    .child(label),
            )
            .when(board.max_rounds > 0, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(12.))
                        .text_color(color(theme::MUTE))
                        .child("ronda")
                        .child(div().flex().gap(px(3.)).children(segments))
                        .child(format!("{round} de {}", board.max_rounds)),
                )
            })
    }

    fn desk_card(
        &self,
        board: &Board,
        agent: &Agent,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let now = self.now_ms;
        let l = look(board, agent, now);
        let sub = match agent.cli {
            Some(AgentCli::ClaudeCode) => format!("{} · claude code · {}", agent.role, agent.model),
            _ => {
                let mut text = format!("{} · {}", agent.role, agent.model);
                if let Some(effort) = &agent.effort {
                    text.push_str(&format!(" · {effort}"));
                }
                text
            }
        };
        let stats = if agent.is_director {
            match agent.started_at_ms {
                Some(start) => format!(
                    "encargos {} · en esta sesión {} min",
                    board
                        .messages
                        .iter()
                        .filter(|m| m.from == agent.id && m.to != USER)
                        .count(),
                    ((now - start) / 60_000).max(0)
                ),
                None => "sin sesión a la vista".into(),
            }
        } else {
            format!("turnos {} · {}", agent.turns, tokens(agent.tokens))
        };
        let quota = match agent.cli {
            Some(cli) => shortest_window(&self.quotas, cli).cloned(),
            None => None,
        };
        let bars = board
            .histogram(&agent.id, now, BARS, BAR_MS)
            .into_iter()
            .map(|bar| {
                let (height, tone) = match bar {
                    Bar::Working => (30., theme::SUCCESS),
                    Bar::NeedsYou => (22., theme::WARNING),
                    Bar::Message(sender) => (14., agent_hex(sender)),
                    Bar::Idle => (3., theme::SURFACE_ELEVATED),
                };
                div().flex_1().h(px(height)).bg(color(tone))
            });
        let session = agent.session.clone();
        div()
            .id(SharedString::from(format!("card-{}", agent.id)))
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(16.))
            .bg(color(theme::CANVAS))
            .border_1()
            .border_color(l.border)
            .rounded(px(4.))
            .when(session.is_some(), |d| d.cursor_pointer())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if session.is_some() {
                    this.open_agent(session.clone(), window, cx);
                }
            }))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(color(agent_hex(agent.cli)))
                            .child(agent.cli.map_or("", |c| c.tag())),
                    )
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(color(theme::INK))
                            .child(agent.title.clone()),
                    )
                    .child(
                        div()
                            .flex_grow_1()
                            .min_w(px(0.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_size(px(12.))
                            .text_color(color(theme::MUTE))
                            .child(sub),
                    )
                    .child(
                        div()
                            .text_size(px(16.))
                            .text_color(color(l.marker_hex))
                            .child(l.marker.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_grow_1()
                            .min_w(px(0.))
                            .line_clamp(2)
                            .text_size(px(14.))
                            .text_color(color(l.action_hex))
                            .child(l.action.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(color(theme::MUTE))
                            .child(in_state(agent.since_ms, now)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(3.))
                    .h(px(32.))
                    .children(bars),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .text_size(px(11.))
                    .text_color(color(theme::MUTE))
                    .child(stats)
                    .child(div().flex_grow_1())
                    .child(match quota {
                        Some(q) => {
                            let filled = (q.remaining_percent() / 10.).round() as usize;
                            let low = q.is_low();
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(format!(
                                    "cuota {} ({}){}",
                                    q.limit,
                                    q.window_label(),
                                    if low { " · baja" } else { "" }
                                ))
                                .child(div().flex().gap(px(2.)).children((0..10).map(|i| {
                                    let tone = if i >= filled {
                                        theme::SURFACE_ELEVATED
                                    } else if low {
                                        theme::WARNING
                                    } else {
                                        theme::MUTE
                                    };
                                    div().w(px(8.)).h(px(10.)).bg(color(tone))
                                })))
                        }
                        None => div().child("cuota: sin dato"),
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_counts_are_short() {
        assert_eq!(tokens(950), "950 tok");
        assert_eq!(tokens(115_400), "115k tok");
        assert_eq!(tokens(1_370_000), "1.4M tok");
    }
}
