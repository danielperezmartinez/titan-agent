//! Detail panel of a session.

use gpui::{ClickEvent, Context, div, prelude::*, px};
use titan_agent_core::protocol::Request;
use titan_agent_core::session::{Session, SessionKind};
use titan_agent_core::time::ago;

use crate::theme::{self, color, hairline};

use super::widgets::button;
use super::{MainView, Pending, Selection};

impl MainView {
    pub(super) fn detail(
        &self,
        session: &Session,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let parent = session.parent.as_deref().and_then(|p| self.session(p));
        let launched_by = match parent {
            Some(parent) => format!(
                "{} ({})",
                parent.name,
                parent
                    .model
                    .clone()
                    .unwrap_or_else(|| parent.agent.tag().into())
            ),
            None if session.launched_here => "tú (titan-agent)".into(),
            None if session.kind == SessionKind::Interactive => "tú".into(),
            None => "sin dato del CLI".into(),
        };
        let mut talks: Vec<(String, String, u32, String)> = Vec::new();
        if let Some(parent) = parent {
            talks.push((
                "recibe de".into(),
                format!(
                    "{} · {}",
                    parent
                        .model
                        .clone()
                        .unwrap_or_else(|| parent.agent.tag().into()),
                    parent.role.clone().unwrap_or_default()
                ),
                theme::agent_color(parent.agent),
                parent.name.clone(),
            ));
        }
        for child in self
            .sessions
            .iter()
            .filter(|s| s.parent.as_deref() == Some(&session.key))
        {
            talks.push((
                "encarga a".into(),
                format!(
                    "{} · {}",
                    child
                        .model
                        .clone()
                        .unwrap_or_else(|| child.agent.tag().into()),
                    child.role.clone().unwrap_or_default()
                ),
                theme::agent_color(child.agent),
                child.name.clone(),
            ));
        }
        let field = |label: &'static str, value: String| {
            div()
                .flex()
                .gap(px(12.))
                .child(
                    div()
                        .w(px(120.))
                        .flex_none()
                        .text_color(color(theme::MUTE))
                        .child(label),
                )
                .child(div().flex_1().min_w(px(0.)).child(value))
        };
        let project_root = session.project_root.clone();
        let open_key = session.key.clone();
        let stop_key = session.key.clone();
        let stoppable = session.stoppable;
        div()
            .w(px(300.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(16.))
            .p(px(16.))
            .border_l_1()
            .border_color(hairline())
            .text_size(px(12.))
            .child(div().text_color(color(theme::MUTE)).child("DETALLE"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(field("proyecto", session.project.clone()))
                    .child(field(
                        "rol",
                        session
                            .role
                            .clone()
                            .unwrap_or_else(|| "sesión suelta".into()),
                    ))
                    .child(field("lanzada por", launched_by))
                    .child(field(
                        "estado desde",
                        session
                            .state_since_ms
                            .map(|since| ago(since, self.now_ms))
                            .unwrap_or_else(|| "—".into()),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(div().text_color(color(theme::MUTE)).child("HABLA CON"))
                    .when(talks.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(color(theme::MUTE))
                                .child("nadie por ahora"),
                        )
                    })
                    .children(talks.into_iter().map(|(dir, who, agent, what)| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .p(px(8.))
                            .bg(color(theme::SURFACE))
                            .rounded(px(4.))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .child(div().text_color(color(theme::MUTE)).child(dir))
                                    .child(div().text_color(color(agent)).child(who)),
                            )
                            .child(
                                div()
                                    .text_color(color(theme::MUTE))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(what),
                            )
                    }))
                    .child(
                        div()
                            .id("open-monitor")
                            .text_color(color(theme::ACCENT))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.selection = Selection::Project(project_root.clone());
                                cx.notify();
                            }))
                            .child("[>] ver el monitor del proyecto"),
                    ),
            )
            .child(div().flex_grow_1())
            .when_some(self.notice.clone(), |d, (text, error)| {
                d.child(
                    div()
                        .text_color(color(if error { theme::DANGER } else { theme::MUTE }))
                        .child(text),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(
                        button(
                            "open-folder",
                            "[d] abrir carpeta",
                            theme::HAIRLINE_STRONG,
                            theme::BODY,
                        )
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, _cx| {
                                let id = this.client.request(Request::OpenFolder {
                                    session: open_key.clone(),
                                });
                                this.pending.insert(id, Pending::Open);
                            },
                        )),
                    )
                    .child(
                        button(
                            "stop",
                            "[x] detener",
                            if stoppable {
                                theme::DANGER
                            } else {
                                theme::HAIRLINE_STRONG
                            },
                            if stoppable {
                                theme::DANGER
                            } else {
                                theme::STONE
                            },
                        )
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                if !stoppable {
                                    this.notice = Some((
                                        "el CLI no ofrece un comando para detener esta sesión"
                                            .into(),
                                        true,
                                    ));
                                } else {
                                    let id = this.client.request(Request::Stop {
                                        session: stop_key.clone(),
                                    });
                                    this.pending.insert(id, Pending::Stop);
                                    this.notice = Some(("deteniendo…".into(), false));
                                }
                                cx.notify();
                            },
                        )),
                    ),
            )
    }
}
