//! A team role's Codex thread, read-only: the transcript the daemon reads
//! from the official app-server (request `transcript`), which opens no
//! writer, so the director can still give the role turns. «Tomar el control»
//! is the only way from here to `codex resume`, after a warning.

use gpui::{
    AnyElement, ClickEvent, Context, Div, FontWeight, ScrollHandle, SharedString, div, prelude::*,
    px,
};
use serde_json::json;
use titan_agent_core::discovery::codex_session;
use titan_agent_core::protocol::{ErrorBody, Request, TranscriptResult};
use titan_agent_core::session::{AgentCli, Session};
use titan_agent_core::transcript::{TranscriptItem, TranscriptKind};

use crate::terminal::TerminalView;
use crate::theme::{self, color, hairline};

use super::widgets::{button, marked};
use super::{MainView, Pending, Selection};

/// The transcript is asked again this often while it is on screen.
const REFRESH_MS: i64 = 3_000;
/// Width of the prefix column of the transcript.
const PREFIX_W: f32 = 96.;

/// A role of the monitor whose thread can be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ThreadRef {
    /// `cx:<thread>`.
    pub key: String,
    /// Short name in bold (`sol`) and role as shown (`revisor`).
    pub title: String,
    pub role: String,
    pub model: String,
    pub task: String,
}

pub(super) struct ThreadState {
    pub r: ThreadRef,
    result: Option<TranscriptResult>,
    error: Option<ErrorBody>,
    asked_ms: i64,
    waiting: bool,
    confirm: bool,
    scroll: ScrollHandle,
    /// Scroll to the newest item once it is laid out.
    stick: bool,
}

impl MainView {
    /// Shows a role's thread read-only and asks the daemon for it.
    pub(super) fn open_thread(&mut self, r: ThreadRef, cx: &mut Context<Self>) {
        self.notice = None;
        self.selection = Selection::Thread(r.key.clone());
        if self.thread.as_ref().is_none_or(|t| t.r.key != r.key) {
            self.thread = Some(ThreadState {
                r,
                result: None,
                error: None,
                asked_ms: 0,
                waiting: false,
                confirm: false,
                scroll: ScrollHandle::new(),
                stick: true,
            });
        }
        self.ask_transcript();
        cx.notify();
    }

    /// Asks for the transcript again when it is on screen and due.
    pub(super) fn refresh_thread(&mut self) {
        if matches!(self.selection, Selection::Thread(_))
            && self
                .thread
                .as_ref()
                .is_some_and(|t| !t.waiting && self.now_ms - t.asked_ms >= REFRESH_MS)
        {
            self.ask_transcript();
        }
    }

    fn ask_transcript(&mut self) {
        let Some(state) = self.thread.as_mut() else {
            return;
        };
        state.asked_ms = self.now_ms;
        state.waiting = true;
        let id = self.client.request(Request::Transcript {
            session: state.r.key.clone(),
        });
        self.pending
            .insert(id, Pending::Transcript(state.r.key.clone()));
    }

    pub(super) fn transcript_answer(
        &mut self,
        key: &str,
        answer: Result<TranscriptResult, ErrorBody>,
    ) {
        let Some(state) = self.thread.as_mut().filter(|t| t.r.key == key) else {
            return;
        };
        state.waiting = false;
        match answer {
            Ok(result) => {
                let grew = state
                    .result
                    .as_ref()
                    .is_none_or(|old| old.items != result.items);
                state.stick |= grew;
                state.result = Some(result);
                state.error = None;
            }
            Err(error) => state.error = Some(error),
        }
    }

    pub(super) fn cancel_take_control(&mut self) -> bool {
        match self.thread.as_mut().filter(|t| t.confirm) {
            Some(state) => {
                state.confirm = false;
                true
            }
            None => false,
        }
    }

    /// Opens the thread with `codex resume` (the app becomes its writer),
    /// listed or not: the daemon resolves a thread it does not list.
    fn take_control(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.thread.as_mut() else {
            return;
        };
        state.confirm = false;
        let key = state.r.key.clone();
        if !self.terminals.contains_key(&key) {
            let client = self.client.clone();
            let session = key.clone();
            let view = cx.new(|cx| TerminalView::new(session, client, cx));
            self.terminals.insert(key.clone(), view);
            self.attach(&key, cx);
        }
        self.selection = Selection::Session(key);
        self.focus_terminal = true;
        cx.notify();
    }

    /// The session of the thread opened from here, while the daemon does
    /// not list it yet (a finished `codex exec` until `codex resume` loads
    /// it), so its terminal can be shown.
    pub(super) fn thread_session(&self, key: &str) -> Option<Session> {
        let state = self.thread.as_ref().filter(|t| t.r.key == key)?;
        let thread = key.strip_prefix("cx:")?;
        let cwd = state.result.as_ref().and_then(|r| r.cwd.clone());
        codex_session(
            &json!({
                "id": thread,
                "cwd": cwd.unwrap_or_default(),
                "name": format!("{} · {}", state.r.role, state.r.task),
                "model": state.r.model,
                "status": {"type": "idle"},
            }),
            true,
        )
    }

    pub(super) fn thread_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = self.thread.as_mut() else {
            return super::widgets::placeholder("Sin hilo", vec![]).into_any_element();
        };
        if std::mem::take(&mut state.stick) && state.result.is_some() {
            state.scroll.scroll_to_bottom();
        }
        let r = state.r.clone();
        let codex = theme::agent_color(AgentCli::Codex);
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
            .child(div().text_color(color(codex)).child(AgentCli::Codex.tag()))
            .child(
                div()
                    .text_color(color(theme::INK))
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(13.))
                    .whitespace_nowrap()
                    .child(r.title.clone()),
            )
            .child(
                div()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .min_w(px(0.))
                    .child(format!("{} · {} · encargo {}", r.role, r.model, r.task)),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(6.))
                    .border_1()
                    .border_color(hairline())
                    .rounded(px(4.))
                    .text_size(px(11.))
                    .line_height(px(16.))
                    .child("solo lectura"),
            )
            .child(div().flex_grow_1())
            .child(
                button(
                    "take-control",
                    "[>] tomar el control",
                    theme::HAIRLINE_STRONG,
                    theme::BODY,
                )
                .flex_none()
                .min_h(px(28.))
                .hover(|d| {
                    d.border_color(color(theme::ACCENT))
                        .text_color(color(theme::INK))
                })
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    if let Some(state) = this.thread.as_mut() {
                        state.confirm = true;
                    }
                    cx.notify();
                })),
            );
        let confirm = state.confirm.then(|| confirm_strip(&r, cx));
        let body = body(state, &r);
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(bar)
            .when_some(confirm, |d, strip| d.child(strip))
            .child(body)
            .into_any_element()
    }
}

fn confirm_strip(r: &ThreadRef, cx: &mut Context<MainView>) -> Div {
    let thread = r.key.trim_start_matches("cx:").to_string();
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(12.))
        .mx(px(16.))
        .mt(px(12.))
        .px(px(12.))
        .py(px(8.))
        .bg(color(theme::SURFACE))
        .border_1()
        .border_color(color(theme::WARNING))
        .rounded(px(4.))
        .text_size(px(12.))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_grow_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(marked(
                    "[?]",
                    theme::WARNING,
                    format!("Se abrirá codex resume {thread} y la app será quien escribe en el hilo."),
                ))
                .child(div().text_color(color(theme::MUTE)).child(format!(
                    "Mientras siga abierto, el director no podrá encargar turnos a {} (titan-team saldrá con 4).",
                    r.role
                ))),
        )
        .child(
            button("take-ok", "[ok] tomar el control", theme::WARNING, theme::INK)
                .flex_none()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.take_control(cx))),
        )
        .child(
            button("take-cancel", "[esc] cancelar", theme::HAIRLINE_STRONG, theme::MUTE)
                .flex_none()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.cancel_take_control();
                    cx.notify();
                })),
        )
}

fn body(state: &ThreadState, r: &ThreadRef) -> impl IntoElement + use<> {
    let content: Vec<AnyElement> = match (&state.result, &state.error) {
        (_, Some(error)) => vec![message(&error_text(error))],
        (None, None) => vec![message("Leyendo el hilo…")],
        (Some(result), None) if result.items.is_empty() => {
            vec![message("El hilo aún no tiene nada que mostrar.")]
        }
        (Some(result), None) => transcript(result, r),
    };
    div()
        .id("transcript")
        .flex_grow_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .track_scroll(&state.scroll)
        .bg(color(theme::TERMINAL_BG))
        .px(px(16.))
        .py(px(12.))
        .text_size(px(13.))
        .line_height(px(19.))
        .children(content)
}

fn error_text(error: &ErrorBody) -> String {
    match error.code.as_str() {
        "codex_unavailable" => format!(
            "No se puede leer el hilo: el daemon de Codex no está disponible ({}).",
            error.message
        ),
        "not_found" => format!("Codex no encuentra este hilo. {}", error.message),
        _ => format!("No se pudo leer el hilo: {}", error.message),
    }
}

fn message(text: &str) -> AnyElement {
    div()
        .text_color(color(theme::MUTE))
        .child(text.to_string())
        .into_any_element()
}

fn transcript(result: &TranscriptResult, r: &ThreadRef) -> Vec<AnyElement> {
    let mut rows = Vec::new();
    if result.more {
        rows.push(message("… hay elementos anteriores que no se muestran"));
    }
    let mut turn: Option<&str> = None;
    let mut turns = 0;
    for item in &result.items {
        if turn != Some(item.turn.as_str()) {
            turn = Some(item.turn.as_str());
            turns += 1;
            rows.push(separator(turns));
        }
        rows.push(row(item, r));
    }
    rows
}

fn separator(turn: usize) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .mt(px(if turn == 1 { 0. } else { 12. }))
        .mb(px(8.))
        .text_size(px(11.))
        .text_color(color(theme::STONE))
        .child(format!("turno {turn}"))
        .child(div().flex_grow_1().h(px(1.)).bg(hairline()))
        .into_any_element()
}

fn row(item: &TranscriptItem, r: &ThreadRef) -> AnyElement {
    let codex = theme::agent_color(AgentCli::Codex);
    let claude = theme::agent_color(AgentCli::ClaudeCode);
    let (prefix, prefix_hex, text_hex): (SharedString, u32, u32) = match item.kind {
        TranscriptKind::User => ("director ›".into(), claude, theme::BODY),
        TranscriptKind::Agent => (format!("{} ›", r.title).into(), codex, theme::INK),
        TranscriptKind::Reasoning => ("~".into(), theme::STONE, theme::MUTE),
        TranscriptKind::Command => ("$".into(), theme::MUTE, theme::BODY),
        TranscriptKind::FileChange => ("[~]".into(), theme::MUTE, theme::BODY),
        TranscriptKind::Tool => ("[t]".into(), theme::MUTE, theme::BODY),
        TranscriptKind::Other => ("·".into(), theme::STONE, theme::MUTE),
    };
    let detail = item.detail.clone().map(|detail| {
        let hex = if detail == "código 0" {
            theme::SUCCESS
        } else {
            theme::DANGER
        };
        div()
            .flex_none()
            .text_size(px(12.))
            .text_color(color(hex))
            .child(detail)
    });
    div()
        .flex()
        .gap(px(12.))
        .py(px(3.))
        .child(
            div()
                .w(px(PREFIX_W))
                .flex_none()
                .text_color(color(prefix_hex))
                .whitespace_nowrap()
                .overflow_hidden()
                .child(prefix),
        )
        .child(
            div()
                .flex_grow_1()
                .min_w(px(0.))
                .text_color(color(text_hex))
                .child(item.text.clone()),
        )
        .when_some(detail, |d, detail| d.child(detail))
        .into_any_element()
}
