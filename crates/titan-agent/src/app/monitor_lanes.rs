//! Lanes view of the monitor (`MonitorB.dc.html`): one row per agent with
//! its state on the left; on the right time runs to the left: green bars
//! while it works, amber while it needs you, and a vertical line per message
//! in the sender's colour with a dot on the receiver.

use gpui::{
    ClickEvent, Context, Div, ElementId, FontWeight, Rgba, SharedString, div, prelude::*, px,
    relative,
};
use titan_agent_core::monitor::{AgentStatus, Monitor, YOU};

use crate::theme::{self, color, hairline};

use super::MainView;
use super::monitor::{LANE_WINDOWS, agent_tag, agent_tint, pulse, status_mark};

const LANE_H: f32 = 72.;
/// Space between the «ahora» line and the right border.
const NOW_GAP: f32 = 40.;

fn faint(hex: u32, alpha: f32) -> Rgba {
    let mut c = color(hex);
    c.a = alpha;
    c
}

/// Row separators: rgba(253,252,252,0.08).
fn row_line() -> Rgba {
    faint(theme::INK, 0.08)
}

impl MainView {
    pub(super) fn lanes_view(&self, monitor: &Monitor, cx: &mut Context<Self>) -> Div {
        let (window_ms, window_label) = LANE_WINDOWS[self.lanes_window];
        let now = monitor.now_ms;
        let x_of = |at_ms: i64| 1. - (now - at_ms) as f32 / window_ms as f32;
        let count = monitor.agents.len();
        let height = LANE_H * count as f32;

        // Left column: who and what.
        let column = div()
            .w(px(240.))
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(hairline())
            .children((0..count).map(|i| self.lane_label(monitor, i, cx)));

        // Right: the moving time line.
        let mut track = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right(px(NOW_GAP));
        for segment in &monitor.segments {
            let x0 = x_of(segment.from_ms).max(0.);
            let x1 = x_of(segment.to_ms).min(1.);
            if x1 <= 0. || x0 >= 1. || x1 <= x0 {
                continue;
            }
            let (fill, line) = if segment.needs_you {
                (theme::WARNING, theme::WARNING)
            } else {
                (theme::SUCCESS, theme::SUCCESS)
            };
            let blinking = segment.needs_you && segment.to_ms >= now;
            track = track.child(
                div()
                    .absolute()
                    .left(relative(x0))
                    .w(relative(x1 - x0))
                    .top(px(segment.agent as f32 * LANE_H + 25.))
                    .h(px(22.))
                    .bg(faint(fill, 0.28))
                    .border_t_2()
                    .border_color(color(line))
                    .when(blinking, |d| d.opacity(pulse(now, 1200, 0.4))),
            );
        }
        for message in &monitor.messages {
            let x = x_of(message.at_ms);
            if !(0. ..=1.).contains(&x) {
                continue;
            }
            let (a, b) = (message.from, message.to);
            let y1 = a.min(b) as f32 * LANE_H + 36.;
            let y2 = a.max(b) as f32 * LANE_H + 36.;
            let down = b > a;
            let tint = color(agent_tint(&monitor.agents[message.from]));
            let dot_y = if down { y2 } else { y1 };
            let label_y = if down { y1 } else { y2 };
            // A pin at the message's time; its children are placed in px.
            track = track.child(
                div()
                    .absolute()
                    .left(relative(x))
                    .top_0()
                    .w(px(0.))
                    .h(px(height))
                    .child(
                        div()
                            .absolute()
                            .left(px(-1.))
                            .top(px(y1))
                            .w(px(2.))
                            .h(px(y2 - y1))
                            .bg(tint),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(-5.))
                            .top(px(dot_y - 5.))
                            .size(px(10.))
                            .rounded_full()
                            .bg(tint),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(6.))
                            .top(px(label_y - 14.))
                            .text_size(px(11.))
                            .text_color(tint)
                            .whitespace_nowrap()
                            .child(message.label.clone()),
                    ),
            );
        }
        let area = div()
            .relative()
            .flex_1()
            .min_w(px(400.))
            .h(px(height))
            .overflow_hidden()
            .children((0..count).map(|i| {
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(i as f32 * LANE_H))
                    .h(px(LANE_H))
                    .border_b_1()
                    .border_color(row_line())
            }))
            .child(track)
            .child(
                div()
                    .absolute()
                    .right(px(NOW_GAP))
                    .top_0()
                    .bottom_0()
                    .w(px(1.))
                    .bg(color(theme::ACCENT)),
            )
            .child(
                div()
                    .absolute()
                    .right(px(NOW_GAP + 6.))
                    .top(px(4.))
                    .text_size(px(11.))
                    .text_color(color(theme::ACCENT))
                    .child("ahora"),
            );

        let legend_box = |fill: u32| {
            div()
                .w(px(24.))
                .h(px(10.))
                .bg(faint(fill, 0.28))
                .border_t_2()
                .border_color(color(fill))
        };
        let legend = div()
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
                    .child(legend_box(theme::SUCCESS))
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
                    .child(legend_box(theme::WARNING))
                    .child("te necesita"),
            )
            .child("el tiempo corre hacia la izquierda · clic en un carril: abre su sesión")
            .child(
                div()
                    .id("lanes-window")
                    .cursor_pointer()
                    .text_color(color(theme::BODY))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.lanes_window = (this.lanes_window + 1) % LANE_WINDOWS.len();
                        cx.notify();
                    }))
                    .child(format!("ventana {window_label} [v]")),
            );

        div()
            .flex()
            .flex_col()
            .flex_grow_1()
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
                            .child(monitor.headline()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .border_1()
                    .border_color(hairline())
                    .bg(color(theme::CANVAS))
                    .child(column)
                    .child(area),
            )
            .child(legend)
    }

    fn lane_label(
        &self,
        monitor: &Monitor,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let agent = &monitor.agents[index];
        let tint = agent_tint(agent);
        let current = monitor.current_message();
        let (mut mark, mut mark_color) = status_mark(agent.status, monitor.now_ms);
        if current.is_some_and(|m| m.from == index) && index != YOU {
            mark = "[>]".into();
            mark_color = tint;
        }
        let action = if index == YOU {
            if current.is_some_and(|m| m.to == YOU) || agent.status == AgentStatus::NeedsYou {
                "te piden una decisión".to_string()
            } else {
                String::new()
            }
        } else {
            monitor.action_now(index, true)
        };
        let active = agent.status != AgentStatus::Waiting
            || current.is_some_and(|m| m.from == index || m.to == index);
        let action_color = match agent.status {
            AgentStatus::NeedsYou => 0xffb340,
            _ if active => theme::BODY,
            _ => theme::MUTE,
        };
        let session = agent.session.clone();
        div()
            .id(ElementId::Name(SharedString::from(format!(
                "lane-{}",
                agent.id
            ))))
            .h(px(LANE_H))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(4.))
            .px(px(12.))
            .py(px(10.))
            .border_b_1()
            .border_color(row_line())
            .when(session.is_some(), |d| d.cursor_pointer())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.open_agent(session.clone(), cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .child(div().text_color(color(tint)).child(agent_tag(agent)))
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
                            .text_size(px(11.))
                            .text_color(color(theme::MUTE))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(agent.label.clone()),
                    )
                    .child(div().text_color(color(mark_color)).child(mark)),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(color(action_color))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(action),
            )
    }
}
