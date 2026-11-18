//! The conversation view in its session state: the session's bar and, in
//! 0.7.0, its embedded terminal with the detail beside it (the chat comes in
//! 0.8.0). In a director session the bar has `[m] monitor`, which puts the
//! monitor of its task in place of the terminal.

use gpui::{
    AnyElement, ClickEvent, Context, Div, FontWeight, SharedString, Stateful, Window, div,
    prelude::*, px,
};
use titan_agent::conversation::SessionBody;
use titan_agent_core::project::tilde;
use titan_agent_core::session::{AgentCli, Session, SessionKind};

use crate::theme::{self, color, hairline};

use super::MainView;
use super::widgets::{marked, state_marker};

impl MainView {
    pub(super) fn session_view(
        &mut self,
        session: &Session,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let agent = theme::agent_color(session.agent);
        let model = session.model.clone().unwrap_or_else(|| "modelo ?".into());
        let director = !self.director_tasks(&session.key).is_empty();
        let monitor = matches!(self.body, SessionBody::Monitor { .. });
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
            .overflow_hidden()
            .child(
                div()
                    .text_color(color(theme::INK))
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(13.))
                    .whitespace_nowrap()
                    .child(session.name.clone()),
            )
            .child(
                div()
                    .text_color(color(theme::state_color(session.state)))
                    .whitespace_nowrap()
                    .child(format!(
                        "{} {}",
                        state_marker(session.state, self.now_ms),
                        session.state.label()
                    )),
            )
            .child(
                div()
                    .text_color(color(agent))
                    .whitespace_nowrap()
                    .child(format!("{} · {}", session.agent, model)),
            )
            .child(
                div()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .min_w(px(0.))
                    .child(tilde(&session.cwd, self.home.as_deref())),
            )
            .child(div().flex_grow_1())
            .child(
                div()
                    .text_color(color(theme::STONE))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .min_w(px(0.))
                    .flex_shrink(1.)
                    .child(match session.attach_line() {
                        Some(line) => format!("$ {line}"),
                        None => "sin comando para adjuntar".into(),
                    }),
            )
            .when(director, |d| {
                let key = session.key.clone();
                d.child(bar_button("bar-monitor", "[m] monitor", monitor).on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_monitor(&key, cx)),
                ))
            })
            .child(
                bar_button("bar-terminal", "[t] terminal", !monitor).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.body = SessionBody::Terminal;
                        this.task_menu = false;
                        cx.notify();
                    },
                )),
            );
        if let SessionBody::Monitor { task } = self.body.clone() {
            return div().flex().flex_col().size_full().child(bar).child(
                div()
                    .flex_grow_1()
                    .min_h(px(0.))
                    .child(self.director_monitor(session, &task, cx)),
            );
        }

        let terminal: AnyElement = match self.terminals.get(&session.key).cloned() {
            Some(view) => {
                let ended = view.read(cx).ended.clone();
                let key = session.key.clone();
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .child(div().flex_grow_1().min_h(px(0.)).child(view))
                    .when_some(ended, |d, ended| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .mt(px(8.))
                                .px(px(12.))
                                .py(px(8.))
                                .border_1()
                                .border_color(color(theme::HAIRLINE_STRONG))
                                .rounded(px(4.))
                                .text_size(px(12.))
                                .text_color(color(theme::MUTE))
                                .child(marked("[x]", theme::MUTE, ended))
                                .child(
                                    div()
                                        .id("reattach")
                                        .text_color(color(theme::ACCENT))
                                        .cursor_pointer()
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, _, cx| {
                                                this.reopen(&key, cx)
                                            },
                                        ))
                                        .child("[>] volver a adjuntar"),
                                ),
                        )
                    })
                    .into_any_element()
            }
            None => not_attachable(session).into_any_element(),
        };

        div().flex().flex_col().size_full().child(bar).child(
            div()
                .flex()
                .flex_grow_1()
                .min_h(px(0.))
                .child(
                    div()
                        .flex_grow_1()
                        .min_w(px(0.))
                        .bg(color(theme::TERMINAL_BG))
                        .p(px(16.))
                        .child(terminal),
                )
                .child(self.detail(session, cx)),
        )
    }
}

/// A switch of the session's bar; the active one has an `accent` border.
fn bar_button(id: &'static str, label: &'static str, active: bool) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .flex()
        .flex_none()
        .items_center()
        .h(px(28.))
        .px(px(10.))
        .border_1()
        .rounded(px(4.))
        .border_color(if active {
            color(theme::ACCENT).into()
        } else {
            gpui::Hsla::from(hairline())
        })
        .when(active, |d| d.bg(color(theme::SURFACE_ELEVATED)))
        .text_color(color(if active { theme::INK } else { theme::BODY }))
        .whitespace_nowrap()
        .cursor_pointer()
        .child(label)
}

fn not_attachable(session: &Session) -> Div {
    let mut lines = Vec::new();
    let mut title = "Sin terminal adjuntable";
    if session.key.starts_with("launch:") {
        title = "Lanzando la sesión";
        lines.push(format!("Esperando a que {} la registre…", session.agent));
        lines.push("Su terminal aparece aquí en cuanto el CLI la muestra.".into());
    } else {
        match (session.agent, session.kind) {
            (AgentCli::ClaudeCode, SessionKind::Interactive) => {
                lines.push(match session.pid {
                    Some(pid) => {
                        format!(
                            "Es una sesión interactiva: vive en su propio terminal (pid {pid})."
                        )
                    }
                    None => "Es una sesión interactiva: vive en su propio terminal.".into(),
                });
                lines.push(
                    "claude attach solo adjunta sesiones en segundo plano (claude --bg).".into(),
                );
            }
            _ => lines.push("El CLI no ofrece un comando para adjuntar esta sesión.".into()),
        }
    }
    div()
        .flex()
        .flex_col()
        .size_full()
        .items_center()
        .justify_center()
        .gap(px(12.))
        .child(
            div()
                .text_color(color(theme::INK))
                .text_size(px(20.))
                .font_weight(FontWeight::BOLD)
                .child(title),
        )
        .children(lines.into_iter().map(|line| {
            div()
                .text_color(color(theme::MUTE))
                .text_size(px(13.))
                .child(line)
        }))
}
