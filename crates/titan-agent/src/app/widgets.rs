//! Small pieces shared by several views.

use gpui::{Div, FontWeight, SharedString, Stateful, div, prelude::*, px};
use titan_agent_core::session::{AgentCli, SessionState};

use crate::theme::{self, color};

/// ASCII marker of a state. The spinner `| / - \` belongs to the monitor
/// (phase 3); the tree and the bar show the plain marker.
pub(super) fn state_marker(state: SessionState, _now_ms: i64) -> String {
    state.marker().to_string()
}

pub(super) fn marked(marker: &str, marker_color: u32, text: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .gap(px(6.))
        .child(
            div()
                .text_color(color(marker_color))
                .child(marker.to_string()),
        )
        .child(text.into())
}

pub(super) fn caret() -> Div {
    div().w(px(1.)).h(px(14.)).bg(color(theme::ACCENT))
}

pub(super) fn agent_pill(agent: AgentCli, model: Option<&str>, agent_color: u32) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .px(px(4.))
        .border_1()
        .border_color(color(agent_color))
        .rounded(px(4.))
        .text_size(px(11.))
        .line_height(px(16.))
        .text_color(color(agent_color))
        .child(agent.tag())
        .when_some(model.map(str::to_string), |d, model| {
            d.child(div().text_color(color(theme::MUTE)).child(model))
        })
}

pub(super) fn button(
    id: &'static str,
    label: &'static str,
    border: u32,
    text: u32,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .min_h(px(32.))
        .px(px(12.))
        .border_1()
        .border_color(color(border))
        .rounded(px(4.))
        .text_size(px(12.))
        .text_color(color(text))
        .cursor_pointer()
        .child(label)
}

pub(super) fn placeholder(title: &str, lines: Vec<String>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_grow_1()
        .size_full()
        .items_center()
        .justify_center()
        .gap(px(12.))
        .bg(color(theme::TERMINAL_BG))
        .px(px(32.))
        .child(
            div()
                .text_color(color(theme::INK))
                .text_size(px(20.))
                .font_weight(FontWeight::BOLD)
                .child(title.to_string()),
        )
        .children(lines.into_iter().map(|line| {
            div()
                .text_color(color(theme::MUTE))
                .text_size(px(13.))
                .child(line)
        }))
}
