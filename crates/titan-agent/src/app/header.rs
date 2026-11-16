//! Header bar: title, state counters, daemon status and notices.

use gpui::{ClickEvent, Context, FontWeight, Role, div, prelude::*, px};
use titan_agent_core::session::StateCounts;

use crate::theme::{self, color, hairline};

use super::widgets::marked;
use super::{MainView, Selection};

impl MainView {
    pub(super) fn header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let counts = StateCounts::tally(self.sessions.iter().map(|s| s.state));
        let link = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .min_h(px(32.))
                .px(px(4.))
                .flex()
                .items_center()
                .text_size(px(14.))
                .text_color(color(theme::BODY))
                .cursor_pointer()
                .child(label)
        };
        div()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(16.))
            .h(px(48.))
            .flex_none()
            .border_b_1()
            .border_color(hairline())
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(8.))
                    .flex_grow_1()
                    .child(
                        div()
                            .text_color(color(theme::INK))
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(16.))
                            .child("titan-agent"),
                    )
                    .child(
                        div()
                            .text_color(color(theme::MUTE))
                            .text_size(px(12.))
                            .child("torre de control"),
                    ),
            )
            .when(!self.connected, |this| {
                this.child(
                    div()
                        .text_size(px(12.))
                        .text_color(color(theme::MUTE))
                        .child(marked(
                            "[x]",
                            theme::DANGER,
                            self.status.clone().unwrap_or_default(),
                        )),
                )
            })
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .text_size(px(12.))
                    .text_color(color(theme::MUTE))
                    .child(marked(
                        "[>]",
                        theme::SUCCESS,
                        format!("{} trabajando", counts.working),
                    ))
                    .child(marked(
                        "[?]",
                        theme::WARNING,
                        format!("{} te necesita", counts.needs_you),
                    ))
                    .child(format!("[+] {} en espera", counts.waiting)),
            )
            .child(link("about", "[i]"))
            .child(
                link("settings", "[*]")
                    .role(Role::Button)
                    .aria_label("configuración")
                    .text_color(color(if self.selection == Selection::Settings {
                        theme::ACCENT
                    } else {
                        theme::BODY
                    }))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.selection = Selection::Settings;
                        this.settings_menu = None;
                        cx.notify();
                    })),
            )
    }
}
