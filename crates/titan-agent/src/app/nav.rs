//! Left column: «Nueva sesión», the filter, the session tree by project and its legend.

use gpui::{
    AnyElement, ClickEvent, Context, ElementId, FontWeight, KeyDownEvent, Role, SharedString,
    Window, div, prelude::*, px,
};
use titan_agent_core::tree::{TreeRow, build_rows};

use crate::theme::{self, color, hairline};

use super::widgets::{agent_pill, caret, marked, state_marker};
use super::{MainView, Selection};

impl MainView {
    pub(super) fn filter_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        match keystroke.key.as_str() {
            "backspace" => {
                if keystroke.modifiers.control {
                    self.filter.clear();
                } else {
                    self.filter.pop();
                }
            }
            "escape" => {
                self.filter.clear();
                self.focus.focus(window, cx);
            }
            "enter" | "tab" => {}
            _ if keystroke.modifiers.control || keystroke.modifiers.platform => return,
            _ => match &keystroke.key_char {
                Some(text) => self.filter.push_str(text),
                None => return,
            },
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn nav(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let is_new = self.selection == Selection::New;
        let filter_focused = self.filter_focus.is_focused(window);
        let new_button = div()
            .id("new-session")
            .flex()
            .items_center()
            .gap(px(8.))
            .min_h(px(36.))
            .px(px(12.))
            .border_1()
            .border_color(color(if is_new {
                theme::ACCENT
            } else {
                theme::HAIRLINE_STRONG
            }))
            .rounded(px(4.))
            .bg(color(if is_new {
                theme::SURFACE
            } else {
                theme::SURFACE_ELEVATED
            }))
            .text_color(color(theme::INK))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .role(Role::Button)
            .aria_label("Nueva sesión")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_new(window, cx);
            }))
            .child("[+]")
            .child(div().flex_grow_1().child("Nueva sesión"))
            .child(
                div()
                    .text_color(color(theme::MUTE))
                    .text_size(px(11.))
                    .font_weight(FontWeight::NORMAL)
                    .child("ctrl+n"),
            );

        let filter_text = if self.filter.is_empty() {
            div()
                .text_color(color(theme::MUTE))
                .child("filtrar por nombre, agente o proyecto")
        } else {
            div()
                .text_color(color(theme::BODY))
                .child(self.filter.clone())
        };
        let filter = div()
            .id("filter")
            .track_focus(&self.filter_focus)
            .on_key_down(cx.listener(Self::filter_key))
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(8.))
            .min_h(px(32.))
            .bg(color(theme::SURFACE))
            .rounded(px(4.))
            .border_1()
            .border_color(if filter_focused {
                color(theme::ACCENT)
            } else {
                color(theme::SURFACE)
            })
            .text_size(px(12.))
            .text_color(color(theme::MUTE))
            .cursor_text()
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.filter_focus.focus(window, cx);
                cx.notify();
            }))
            .child("[/]")
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_grow_1()
                    .overflow_hidden()
                    .map(|d| match (filter_focused, self.filter.is_empty()) {
                        (true, false) => d.child(filter_text).child(caret()),
                        (true, true) => d.child(caret()).child(filter_text),
                        (false, _) => d.child(filter_text),
                    }),
            );

        let rows = build_rows(&self.sessions, &self.filter);
        let mut tree = div()
            .id("tree")
            .flex()
            .flex_col()
            .flex_grow_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .pb(px(12.));
        if rows.is_empty() {
            tree = tree.child(
                div()
                    .px(px(12.))
                    .pt(px(8.))
                    .text_size(px(12.))
                    .text_color(color(theme::MUTE))
                    .child(if !self.connected {
                        "sin conexión con titan-agentd".to_string()
                    } else if self.filter.is_empty() {
                        "no hay sesiones abiertas".to_string()
                    } else {
                        format!("nada coincide con «{}»", self.filter)
                    }),
            );
        }
        for row in rows {
            tree = tree.child(self.tree_row(row, cx));
        }

        div()
            .w(px(340.))
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(hairline())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .p(px(12.))
                    .child(new_button)
                    .child(filter)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .pt(px(4.))
                            .text_size(px(12.))
                            .child(
                                div()
                                    .text_color(color(theme::INK))
                                    .font_weight(FontWeight::BOLD)
                                    .child("SESIONES"),
                            )
                            .child(
                                div()
                                    .text_color(color(theme::MUTE))
                                    .child("por proyecto [v]"),
                            ),
                    ),
            )
            .child(tree)
            .child(self.legend())
    }

    fn tree_row(&self, row: TreeRow, cx: &mut Context<Self>) -> AnyElement {
        match row {
            TreeRow::Project {
                name,
                root,
                count,
                team_live,
            } => {
                let selected = self.selection == Selection::Project(root.clone());
                // A live team: nested sessions, or a task of the team tool.
                let team_live = team_live || self.active_team(&root).is_some();
                let id = ElementId::Name(SharedString::from(format!("p:{root}")));
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .w_full()
                    .min_h(px(32.))
                    .mt(px(8.))
                    .px(px(12.))
                    .border_l_2()
                    .border_color(if selected {
                        color(theme::ACCENT)
                    } else {
                        gpui::transparent_black().into()
                    })
                    .when(selected, |d| d.bg(color(theme::SURFACE)))
                    .when(!selected, |d| {
                        d.hover(|style| style.bg(color(theme::CANVAS).blend(hairline())))
                    })
                    .text_size(px(12.))
                    .text_color(color(theme::MUTE))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.selection = Selection::Project(root.clone());
                        this.notice = None;
                        cx.notify();
                    }))
                    .child("[#]")
                    .child(
                        div()
                            .flex_shrink(1.)
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(color(if selected { theme::INK } else { theme::BODY }))
                            .font_weight(FontWeight::MEDIUM)
                            .child(name),
                    )
                    .child(div().flex_grow_1().min_w(px(8.)).h(px(1.)).bg(hairline()))
                    .when(team_live, |d| {
                        d.child(div().text_color(color(theme::SUCCESS)).child("[*]"))
                    })
                    .child(div().flex_none().child(count.to_string()))
                    .into_any_element()
            }
            TreeRow::Session { key, depth, branch } => {
                let Some(session) = self.session(&key) else {
                    return div().into_any_element();
                };
                let selected = self.selection == Selection::Session(key.clone());
                let finished = !session.state.is_live();
                let agent = theme::agent_color(session.agent);
                let indent = if depth == 0 {
                    12.
                } else {
                    12. + 8. * depth as f32
                };
                let id = ElementId::Name(SharedString::from(format!("s:{key}")));
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .w_full()
                    .min_h(px(36.))
                    .pl(px(indent))
                    .pr(px(12.))
                    .border_l_2()
                    .border_color(if selected {
                        color(theme::ACCENT)
                    } else {
                        gpui::transparent_black().into()
                    })
                    .when(selected, |d| d.bg(color(theme::SURFACE)))
                    .when(!selected, |d| {
                        d.hover(|style| style.bg(color(theme::CANVAS).blend(hairline())))
                    })
                    .text_size(px(13.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.select_session(key.clone(), Some(window), cx);
                    }))
                    .when(!branch.is_empty(), |d| {
                        d.child(div().text_color(color(theme::STONE)).child(branch))
                    })
                    .child(
                        div()
                            .text_color(color(theme::state_color(session.state)))
                            .child(state_marker(session.state, self.now_ms)),
                    )
                    .child(
                        div()
                            .flex_grow_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(color(if finished { theme::MUTE } else { theme::BODY }))
                            .child(session.name.clone()),
                    )
                    .child(agent_pill(session.agent, session.model.as_deref(), agent))
                    .into_any_element()
            }
        }
    }

    fn legend(&self) -> impl IntoElement + use<> {
        let failing: Vec<String> = self
            .sources
            .iter()
            .filter(|s| !s.ok)
            .map(|s| {
                format!(
                    "{}: {}",
                    s.agent.tag(),
                    s.detail.clone().unwrap_or_else(|| "sin datos".into())
                )
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .border_t_1()
            .border_color(hairline())
            .text_size(px(11.))
            .text_color(color(theme::MUTE))
            .children(
                failing
                    .into_iter()
                    .map(|text| marked("[x]", theme::DANGER, text)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(12.))
                    .gap_y(px(4.))
                    .child(marked("cl", theme::CLAUDE, "Claude Code"))
                    .child(marked("cx", theme::CODEX, "Codex"))
                    .child(marked("[>]", theme::SUCCESS, "trabajando"))
                    .child(marked("[?]", theme::WARNING, "te necesita"))
                    .child("[+] en espera")
                    .child("[x] terminada"),
            )
    }
}
