//! Lanes view of the monitor (preview `MonitorB`): one row per agent with
//! its state on the left and, on the right, time running to the left:
//! green bars while working, amber while it needs you, vertical lines for
//! messages with a dot on the receiver, and the `accent` «ahora» line.

use gpui::{
    AnyElement, ClickEvent, Context, Div, FontWeight, SharedString, div, prelude::*, px, relative,
    rgba,
};
use titan_agent_core::monitor::Board;

use crate::theme::{self, color, hairline, hsla};

use super::super::MainView;
use super::{LANE_WINDOWS, agent_hex, look, with_alpha};

const LANE_H: f32 = 72.;
/// Width of the session tree on the left of the window.
const NAV_W: f32 = 340.;
/// Space between the «ahora» line and the right edge of the track.
const NOW_MARGIN: f32 = 40.;

impl MainView {
    pub(super) fn lanes_view(&self, board: &Board, cx: &mut Context<Self>) -> Div {
        let now = self.now_ms;
        let window_ms = self.lane_window_s * 1000;
        let start = now - window_ms;
        // Fraction of the track (0 left, 1 «ahora») of a time.
        let at = |ms: i64| ((ms - start) as f32 / window_ms as f32).clamp(0., 1.);
        let row_of = |id: &str| board.agents.iter().position(|a| a.id == id);

        let rows = board.agents.iter().map(|agent| {
            let l = look(board, agent, now);
            let session = agent.session.clone();
            div()
                .id(SharedString::from(format!("lane-{}", agent.id)))
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(4.))
                .h(px(LANE_H))
                .px(px(12.))
                .border_b_1()
                .border_color(with_alpha(theme::INK, 0.08))
                .when(session.is_some(), |d| d.cursor_pointer())
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    if session.is_some() {
                        this.open_agent(session.clone(), window, cx);
                    }
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(13.))
                        .child(
                            div()
                                .w(px(16.))
                                .text_color(color(agent_hex(agent.cli)))
                                .child(agent.cli.map_or("", |c| c.tag())),
                        )
                        .child(
                            div()
                                .text_color(color(theme::INK))
                                .font_weight(FontWeight::BOLD)
                                .child(agent.title.clone()),
                        )
                        .child(
                            div()
                                .flex_grow_1()
                                .min_w(px(0.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(11.))
                                .text_color(color(theme::MUTE))
                                .child(agent.role.clone()),
                        )
                        .child(
                            div()
                                .text_color(color(l.marker_hex))
                                .child(l.marker.clone()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(color(l.action_hex))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(if agent.cli.is_none() && l.dim {
                            String::new()
                        } else {
                            l.action
                        }),
                )
        });

        let mut track: Vec<AnyElement> = Vec::new();
        for span in &board.spans {
            let end = span.end_ms.unwrap_or(now);
            let Some(row) = row_of(&span.agent) else {
                continue;
            };
            if end < start {
                continue;
            }
            let (from, to) = (at(span.start_ms), at(end));
            let (fill, line) = if span.needs_you {
                (rgba(0xff9f0a47), theme::WARNING)
            } else {
                (rgba(0x30d15847), theme::SUCCESS)
            };
            track.push(
                div()
                    .absolute()
                    .left(relative(from))
                    .w(relative((to - from).max(0.004)))
                    .top(px(row as f32 * LANE_H + 25.))
                    .h(px(22.))
                    .bg(fill)
                    .border_t_2()
                    .border_color(color(line))
                    .into_any_element(),
            );
        }
        // Track width in pixels (window minus the tree, padding, the lane
        // column and the margin of «ahora»), to fit each label in the room
        // left before the next message line.
        let track_px = (self.viewport_w - NAV_W - 32. - 240. - 3. - NOW_MARGIN).max(100.);
        let shown: Vec<_> = board
            .messages
            .iter()
            .filter(|m| m.at_ms >= start && m.at_ms <= now)
            .collect();
        for (i, m) in shown.iter().enumerate() {
            let x = at(m.at_ms) * track_px;
            let next = shown
                .get(i + 1)
                .map_or(track_px, |n| at(n.at_ms) * track_px);
            let room = (next.min(track_px) - x - 12.).max(0.);
            let (Some(a), Some(b)) = (row_of(&m.from), row_of(&m.to)) else {
                continue;
            };
            let y1 = a.min(b) as f32 * LANE_H + 36.;
            let y2 = a.max(b) as f32 * LANE_H + 36.;
            let receiver_y = b as f32 * LANE_H + 36.;
            let sender_y = a as f32 * LANE_H + 36.;
            let tone = hsla(agent_hex(m.sender));
            // A zero-width anchor at the message's time; its children are
            // placed in pixels around it.
            track.push(
                div()
                    .absolute()
                    .left(relative(at(m.at_ms)))
                    .top(px(0.))
                    .w(px(0.))
                    .h_full()
                    .child(
                        div()
                            .absolute()
                            .left(px(-1.))
                            .top(px(y1))
                            .w(px(2.))
                            .h(px(y2 - y1))
                            .bg(tone),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(-5.))
                            .top(px(receiver_y - 5.))
                            .size(px(10.))
                            .rounded_full()
                            .bg(tone),
                    )
                    .when(room >= 24., |d| {
                        d.child(
                            div()
                                .absolute()
                                .left(px(6.))
                                .top(px(if b > a { sender_y - 6. } else { sender_y - 12. }))
                                .w(px(room.min(220.)))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_size(px(11.))
                                .text_color(tone)
                                // The colour and the dot already say it arrived; the
                                // label keeps the words that matter.
                                .child(m.text.trim_start_matches("[ok] ").to_string()),
                        )
                    })
                    .into_any_element(),
            );
        }

        let height = board.agents.len() as f32 * LANE_H;
        let window_label = match self.lane_window_s {
            s if s < 60 => format!("ventana {s} s"),
            s => format!("ventana {} min", s / 60),
        };
        let legend_box = |fill: u32, line: u32| {
            div()
                .w(px(24.))
                .h(px(10.))
                .bg(rgba(fill))
                .border_t_2()
                .border_color(color(line))
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .gap(px(24.))
            .px(px(16.))
            .py(px(24.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(color(theme::MUTE))
                            .child("AHORA"),
                    )
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(color(theme::INK))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(board.headline(now)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .border_1()
                    .border_color(hairline())
                    .bg(color(theme::CANVAS))
                    .child(
                        div()
                            .w(px(240.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .border_r_1()
                            .border_color(hairline())
                            .children(rows),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_grow_1()
                            .min_w(px(0.))
                            .h(px(height))
                            .overflow_hidden()
                            .children((0..board.agents.len()).map(|i| {
                                div()
                                    .absolute()
                                    .left(px(0.))
                                    .right(px(0.))
                                    .top(px(i as f32 * LANE_H))
                                    .h(px(LANE_H))
                                    .border_b_1()
                                    .border_color(with_alpha(theme::INK, 0.08))
                            }))
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.))
                                    .right(px(NOW_MARGIN))
                                    .top(px(0.))
                                    .h(px(height))
                                    .children(track),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .right(px(NOW_MARGIN))
                                    .top(px(0.))
                                    .w(px(1.))
                                    .h(px(height))
                                    .bg(color(theme::ACCENT)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .right(px(NOW_MARGIN + 6.))
                                    .top(px(4.))
                                    .text_size(px(11.))
                                    .text_color(color(theme::ACCENT))
                                    .child("ahora"),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(24.))
                    .text_size(px(12.))
                    .text_color(color(theme::MUTE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(legend_box(0x30d15847, theme::SUCCESS))
                            .child("trabajando"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(div().w(px(2.)).h(px(14.)).bg(color(theme::CLAUDE)))
                            .child("mensaje (color de quien envía; el punto marca a quien llega)"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(legend_box(0xff9f0a47, theme::WARNING))
                            .child("te necesita"),
                    )
                    .child("el tiempo corre hacia la izquierda · clic en un carril: abre su sesión")
                    .child(
                        div()
                            .id("lane-window")
                            .text_color(color(theme::ACCENT))
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                let next = LANE_WINDOWS
                                    .iter()
                                    .position(|w| *w == this.lane_window_s)
                                    .map_or(0, |i| (i + 1) % LANE_WINDOWS.len());
                                this.lane_window_s = LANE_WINDOWS[next];
                                cx.notify();
                            }))
                            .child(format!("[{window_label}]")),
                    ),
            )
    }
}
