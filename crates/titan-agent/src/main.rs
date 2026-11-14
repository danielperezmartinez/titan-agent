use gpui::{
    App, Bounds, Context, KeyDownEvent, MouseButton, StatefulInteractiveElement, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;
use std::time::Duration;
use titan_agent::{
    client::{ClientError, Connection, connect_or_spawn, daemon_program, default_socket_path},
    keymap::key_from_gpui,
    terminal_model::TerminalModel,
    theme,
};
use titan_agent_core::{
    keys::{Modifiers, encode_key, encode_paste},
    protocol::StopAction,
    protocol::{ClientRequest, DaemonNotification, Session, SessionsResult},
    session::{SessionState, StateCounts},
    tree::build_tree,
};

struct MainWindow {
    status: String,
    sessions: Vec<Session>,
    connection: Option<Connection>,
    selected: Option<String>,
    filter: String,
    filter_focused: bool,
    terminal: Option<TerminalModel>,
    terminal_session: Option<String>,
    terminal_pending: Vec<u8>,
    terminal_focused: bool,
    exited: Option<Option<i32>>,
    stop_error: Option<String>,
}
impl MainWindow {
    fn new() -> Self {
        Self {
            status: "conectando con titan-agentd…".into(),
            sessions: vec![],
            connection: None,
            selected: None,
            filter: String::new(),
            filter_focused: false,
            terminal: None,
            terminal_session: None,
            terminal_pending: Vec::new(),
            terminal_focused: false,
            exited: None,
            stop_error: None,
        }
    }
    fn start(&mut self, cx: &mut Context<Self>) {
        self.status = "conectando con titan-agentd…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                connect_or_spawn(&default_socket_path(), &daemon_program(), Duration::from_secs(5))
                    .and_then(|conn| {
                        let value = conn.request(ClientRequest::SessionsSubscribe)?;
                        let sessions = serde_json::from_value::<SessionsResult>(value)
                            .map_err(|e| ClientError::Protocol(e.to_string()))?.sessions;
                        Ok((conn, sessions))
                    })
            }).await;
            let _ = this.update(cx, |view, cx| match result {
                Ok((conn, sessions)) => {
                    view.sessions = sessions;
                    view.connection = Some(conn.clone());
                    view.status.clear();
                    if let Some(notices) = conn.take_notifications() {
                        let notices = std::sync::Arc::new(std::sync::Mutex::new(notices));
                        cx.spawn(async move |this, cx| loop {
                            let receiver = notices.clone();
                            let notice = cx.background_executor().spawn(async move {
                                receiver.lock().unwrap().recv().ok()
                            }).await;
                            let Some(notice) = notice else {
                                let _ = this.update(cx, |view, cx| {
                                    view.connection = None;
                                    view.sessions.clear();
                                    view.selected = None;
                                    view.terminal = None;
                                    view.terminal_session = None;
                                    view.status = "no se pudo conectar con titan-agentd: conexión cerrada".into();
                                    cx.notify();
                                });
                                break;
                            };
                            let _ = this.update(cx, |view, cx| view.on_notification(notice, cx));
                        }).detach();
                    }
                }
                Err(error) => view.status = format!("no se pudo conectar con titan-agentd: {}", error_text(&error)),
            });
        }).detach();
    }
    fn on_notification(&mut self, n: DaemonNotification, cx: &mut Context<Self>) {
        match n {
            DaemonNotification::SessionsChanged { sessions } => {
                self.sessions = sessions;
                if self
                    .selected
                    .as_ref()
                    .is_some_and(|id| !self.sessions.iter().any(|s| &s.id == id))
                {
                    self.close_terminal(cx);
                }
            }
            DaemonNotification::TerminalOutput { session, data } => {
                if self.terminal_session.as_deref() == Some(&session) {
                    if let Some(t) = &mut self.terminal {
                        t.feed(&data);
                    } else {
                        self.terminal_pending.extend(data);
                    }
                }
            }
            DaemonNotification::TerminalExited { session, code } => {
                if self.terminal_session.as_deref() == Some(&session) {
                    self.exited = Some(code)
                }
            }
        }
        cx.notify()
    }
    fn close_terminal(&mut self, cx: &mut Context<Self>) {
        if let (Some(conn), Some(id)) = (self.connection.clone(), self.terminal_session.take()) {
            cx.background_spawn(async move {
                let _ = conn.request(ClientRequest::TerminalClose { session: id });
            })
            .detach();
        }
        self.terminal = None;
        self.terminal_pending.clear();
        self.terminal_focused = false;
        self.exited = None;
    }
    fn select(&mut self, id: String, cols: u16, rows: u16, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(&id) {
            return;
        }
        self.close_terminal(cx);
        self.selected = Some(id.clone());
        self.stop_error = None;
        self.filter_focused = false;
        let Some(s) = self.sessions.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        if s.attach.is_none() {
            cx.notify();
            return;
        }
        let Some(conn) = self.connection.clone() else {
            return;
        };
        self.terminal_session = Some(id.clone());
        let id2 = id.clone();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    conn.request(ClientRequest::TerminalOpen {
                        session: id2,
                        cols,
                        rows,
                    })
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                if v.selected.as_deref() == Some(&id) {
                    if r.is_ok() {
                        let mut terminal = TerminalModel::new(cols, rows);
                        terminal.feed(&v.terminal_pending);
                        v.terminal_pending.clear();
                        v.terminal = Some(terminal);
                        v.exited = None;
                    } else {
                        v.stop_error = Some(r.err().map(|e| error_text(&e)).unwrap_or_default());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn resize_terminal(&mut self, cols: u16, rows: u16, cx: &mut Context<Self>) {
        let Some(model) = &mut self.terminal else {
            return;
        };
        if model.size() == (cols, rows) {
            return;
        }
        model.resize(cols, rows);
        if let (Some(conn), Some(session)) =
            (self.connection.clone(), self.terminal_session.clone())
        {
            cx.background_spawn(async move {
                let _ = conn.request(ClientRequest::TerminalResize {
                    session,
                    cols,
                    rows,
                });
            })
            .detach();
        }
        cx.notify();
    }
    fn request_input(&self, data: Vec<u8>, cx: &mut Context<Self>) {
        if data.is_empty() {
            return;
        }
        if let (Some(conn), Some(id)) = (self.connection.clone(), self.terminal_session.clone()) {
            cx.background_spawn(async move {
                let _ = conn.request(ClientRequest::TerminalInput { session: id, data });
            })
            .detach();
        }
    }
    fn on_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        let mods = k.modifiers;
        if mods.control && k.key.eq_ignore_ascii_case("q") {
            cx.quit();
            return;
        }
        if self.filter_focused {
            if k.key == "escape" {
                self.filter.clear()
            } else if k.key == "backspace" {
                self.filter.pop();
            } else if !mods.control
                && !mods.platform
                && let Some(ch) =
                    k.key_char.as_deref().or(Some(k.key.as_str())).filter(|s| {
                        s.chars().count() == 1 && !s.chars().next().unwrap().is_control()
                    })
            {
                self.filter.push_str(ch)
            }
            cx.notify();
            return;
        }
        if mods.platform || !self.terminal_focused {
            return;
        }
        if mods.control && mods.shift && k.key.eq_ignore_ascii_case("v") {
            if let Some(item) = cx.read_from_clipboard()
                && let Some(text) = item.text()
            {
                let modes = self
                    .terminal
                    .as_ref()
                    .map(TerminalModel::key_modes)
                    .unwrap_or_default();
                self.request_input(encode_paste(&text, modes), cx);
            }
            return;
        }
        if let Some(key) = key_from_gpui(&k.key, k.key_char.as_deref()) {
            let m = Modifiers {
                shift: mods.shift,
                alt: mods.alt,
                ctrl: mods.control,
            };
            let modes = self
                .terminal
                .as_ref()
                .map(TerminalModel::key_modes)
                .unwrap_or_default();
            if let Some(data) = encode_key(key, m, modes) {
                self.request_input(data, cx)
            }
        }
    }
    fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.selected.clone() else {
            return;
        };
        let Some(conn) = self.connection.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { conn.request(ClientRequest::SessionStop { session: s }) })
                .await;
            let _ = this.update(cx, |v, cx| {
                v.stop_error = r.err().map(|e| error_text(&e));
                cx.notify();
            });
        })
        .detach();
    }
    fn reopen(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected.clone() {
            let (cols, rows) = self.terminal.as_ref().map_or((80, 24), TerminalModel::size);
            self.terminal = None;
            self.terminal_session = None;
            self.exited = None;
            self.selected = None;
            self.select(id, cols, rows, cx)
        }
    }
}
fn is_wide(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x115f | 0x2329..=0x232a | 0x2e80..=0xa4cf | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xfe10..=0xfe19 | 0xfe30..=0xfe6f | 0xff00..=0xff60 | 0xffe0..=0xffe6 | 0x1f300..=0x1faff | 0x20000..=0x3fffd)
}
trait CharacterWidth {
    fn widthless(self) -> bool;
}
impl CharacterWidth for char {
    fn widthless(self) -> bool {
        matches!(self as u32, 0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe00..=0xfe0f)
    }
}
fn error_text(e: &ClientError) -> String {
    match e {
        ClientError::Io(e) => e.to_string(),
        ClientError::Rpc(e) => e.message.clone(),
        ClientError::Protocol(e) => e.clone(),
        ClientError::Closed => "conexión cerrada".into(),
        ClientError::Timeout => "tiempo de conexión agotado".into(),
    }
}
fn state_label(s: SessionState) -> &'static str {
    match s {
        SessionState::Working => "trabajando",
        SessionState::NeedsYou => "te necesita",
        SessionState::Waiting => "en espera",
        SessionState::Finished { .. } => "terminada",
    }
}
fn color(c: u32) -> gpui::Rgba {
    rgb(c)
}
impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let counts = StateCounts::tally(self.sessions.iter().map(|s| s.state));
        let selected = self
            .selected
            .as_ref()
            .and_then(|id| self.sessions.iter().find(|s| &s.id == id))
            .cloned();
        let disappeared = self.selected.is_some() && selected.is_none();
        let mut tree = div()
            .id("session-tree")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for group in build_tree(&self.sessions, &self.filter) {
            tree = tree.child(
                div()
                    .flex()
                    .justify_between()
                    .text_color(color(theme::INK))
                    .child(format!("[#] {}", group.project.name))
                    .child(
                        div()
                            .text_color(color(theme::MUTE))
                            .child(group.counts.total().to_string()),
                    ),
            );
            for row in group.rows {
                let s = self.sessions[row.session].clone();
                let on = self.selected.as_deref() == Some(&s.id);
                let id = s.id.clone();
                let pill = format!(
                    "{}{}",
                    s.cli.tag(),
                    s.model
                        .as_ref()
                        .map(|m| format!(" {m}"))
                        .unwrap_or_default()
                );
                let ac = theme::agent_color(s.cli);
                tree = tree.child(
                    div()
                        .id(format!("session-{}", s.id))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .bg(color(if on { theme::SURFACE } else { theme::CANVAS }))
                        .border_l_2()
                        .border_color(color(if on { theme::ACCENT } else { theme::CANVAS }))
                        .on_click(cx.listener(move |v, _, window, cx| {
                            let bounds = window.bounds().size;
                            let cols = ((f32::from(bounds.width) - 580.0) / 9.0).max(2.0) as u16;
                            let rows = ((f32::from(bounds.height) - 100.0) / 18.0).max(1.0) as u16;
                            v.select(id.clone(), cols, rows, cx)
                        }))
                        .child(div().text_color(color(theme::STONE)).child(row.branch))
                        .child(
                            div()
                                .text_color(color(theme::state_color(s.state)))
                                .child(s.state.marker()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_color(color(theme::BODY))
                                .truncate()
                                .child(s.name),
                        )
                        .child(
                            div()
                                .border_1()
                                .border_color(color(ac))
                                .rounded(px(4.))
                                .px_1()
                                .text_color(color(ac))
                                .child(pill),
                        ),
                );
            }
        }
        let mut center = div().flex_1().flex().flex_col();
        if let Some(s) = selected.clone() {
            center = center.child(
                div()
                    .h(px(46.))
                    .border_b_1()
                    .border_color(gpui::rgba(theme::HAIRLINE_RGBA))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().text_color(color(theme::INK)).child(s.name.clone()))
                    .child(
                        div()
                            .text_color(color(theme::state_color(s.state)))
                            .child(format!("{} {}", s.state.marker(), state_label(s.state))),
                    )
                    .child(
                        div()
                            .text_color(color(theme::agent_color(s.cli)))
                            .child(s.cli.tag().to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_color(color(theme::MUTE))
                            .child(s.cwd.clone()),
                    )
                    .child(
                        div()
                            .text_color(color(theme::STONE))
                            .child(s.attach.as_ref().map(|a| a.join(" ")).unwrap_or_default()),
                    ),
            );
            if s.attach.is_none() {
                center = center.child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .text_color(color(theme::MUTE))
                        .child("sesión interactiva en otro terminal: no se puede adjuntar")
                        .child(format!(
                            "pid {:?} · {} · {}",
                            s.pid,
                            s.cwd,
                            state_label(s.state)
                        )),
                );
            } else if let Some(t) = &self.terminal {
                let mut term = div()
                    .id("terminal")
                    .flex_1()
                    .overflow_hidden()
                    .bg(color(theme::TERMINAL_BG))
                    .p_2()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|v, _, _, cx| {
                            v.terminal_focused = true;
                            v.filter_focused = false;
                            cx.notify();
                        }),
                    )
                    .on_key_down(cx.listener(|v, e, _, cx| {
                        cx.stop_propagation();
                        v.on_key(e, cx);
                    }));
                let lines = t.lines();
                for (ri, line) in lines.iter().enumerate() {
                    let mut ln = div().h(px(18.)).flex().whitespace_nowrap();
                    let mut cell_column = 0u16;
                    let cursor = t
                        .cursor()
                        .filter(|(row, _)| *row == ri as u16)
                        .map(|(_, col)| col);
                    for span in &line.spans {
                        for character in span.text.chars() {
                            let is_cursor = cursor == Some(cell_column);
                            let mut d = div()
                                .text_color(color(if is_cursor {
                                    theme::TERMINAL_BG
                                } else {
                                    span.fg
                                }))
                                .bg(color(if is_cursor {
                                    theme::TERMINAL_CURSOR
                                } else {
                                    span.bg
                                }))
                                .child(character.to_string());
                            if span.bold {
                                d = d.font_weight(gpui::FontWeight::BOLD)
                            }
                            if span.italic {
                                d = d.italic()
                            }
                            if span.underline {
                                d = d.underline()
                            }
                            if span.strikeout {
                                d = d.line_through()
                            }
                            cell_column += if is_wide(character) {
                                2
                            } else if character.widthless() {
                                0
                            } else {
                                1
                            };
                            ln = ln.child(d);
                        }
                    }
                    term = term.child(ln)
                }
                center = center.child(term);
                if let Some(code) = self.exited {
                    center = center.child(
                        div()
                            .h(px(32.))
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_color(color(theme::MUTE))
                            .child(format!(
                                "el proceso terminó (código {})",
                                code.map_or_else(|| "?".into(), |x| x.to_string())
                            ))
                            .child(
                                div()
                                    .id("reopen")
                                    .on_click(cx.listener(|v, _, _, cx| v.reopen(cx)))
                                    .child("[ok] reabrir"),
                            ),
                    );
                }
            } else {
                center = center.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(color(theme::MUTE))
                        .child("abriendo terminal…"),
                );
            }
        } else {
            center = center.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(color(theme::MUTE))
                    .child(if disappeared {
                        "la sesión ya no está".to_string()
                    } else if self.status.is_empty() {
                        "elige una sesión en la columna izquierda".to_string()
                    } else {
                        self.status.clone()
                    }),
            );
            if self.connection.is_none() && self.status.starts_with("no se pudo conectar") {
                center = center.child(
                    div().flex().justify_center().pb_4().child(
                        div()
                            .id("retry")
                            .border_1()
                            .border_color(color(theme::HAIRLINE_STRONG))
                            .rounded(px(4.))
                            .px_2()
                            .py_1()
                            .on_click(cx.listener(|view, _, _, cx| view.start(cx)))
                            .child("[ok] reintentar"),
                    ),
                );
            }
        }
        let mut detail = div()
            .w(px(260.))
            .border_l_1()
            .border_color(gpui::rgba(theme::HAIRLINE_RGBA))
            .flex()
            .flex_col()
            .gap_3()
            .p_3();
        if let Some(s) = selected {
            let parent = s
                .parent
                .as_ref()
                .and_then(|id| self.sessions.iter().find(|x| &x.id == id))
                .map(|x| x.name.as_str())
                .unwrap_or("—")
                .to_string();
            let rows = [
                (
                    "proyecto",
                    format!("{}\n{}", s.project.name, s.project.root),
                ),
                (
                    "agente",
                    format!("{}  {}", s.cli, s.model.as_deref().unwrap_or("")),
                ),
                ("estado", state_label(s.state).into()),
                ("lanzada por", parent),
                ("desde", "—".into()),
                ("pid", s.pid.map_or("—".into(), |p| p.to_string())),
            ];
            for (label, val) in rows {
                detail = detail.child(
                    div()
                        .text_color(color(theme::MUTE))
                        .child(label)
                        .child(div().text_color(color(theme::BODY)).child(val)),
                );
            }
            match s.stop {
                Some(StopAction::Stop) => {
                    detail = detail.child(
                        div()
                            .id("stop")
                            .border_1()
                            .border_color(color(theme::DANGER))
                            .rounded(px(4.))
                            .text_color(color(theme::DANGER))
                            .on_click(cx.listener(|v, _, _, cx| v.stop(cx)))
                            .child("[x] detener"),
                    )
                }
                Some(StopAction::Interrupt) => {
                    detail = detail.child(
                        div()
                            .id("interrupt")
                            .border_1()
                            .border_color(gpui::rgba(theme::HAIRLINE_RGBA))
                            .rounded(px(4.))
                            .text_color(color(theme::BODY))
                            .on_click(cx.listener(|v, _, _, cx| v.stop(cx)))
                            .child("[-] interrumpir"),
                    )
                }
                None => {}
            }
            if let Some(e) = &self.stop_error {
                detail = detail.child(div().text_color(color(theme::DANGER)).child(e.clone()));
            }
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(color(theme::CANVAS))
            .text_color(color(theme::BODY))
            .font_family("JetBrains Mono")
            .on_key_down(cx.listener(|v, e, _, cx| v.on_key(e, cx)))
            .child(
                div()
                    .h(px(48.))
                    .border_b_1()
                    .border_color(gpui::rgba(theme::HAIRLINE_RGBA))
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div().child("titan-agent  ").child(
                            div()
                                .text_color(color(theme::MUTE))
                                .child("torre de control"),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                div()
                                    .text_color(color(theme::SUCCESS))
                                    .child(format!("[>] {} trabajando", counts.working)),
                            )
                            .child(
                                div()
                                    .text_color(color(theme::WARNING))
                                    .child(format!("[?] {} te necesita", counts.needs_you)),
                            )
                            .child(
                                div()
                                    .text_color(color(theme::MUTE))
                                    .child(format!("[+] {} en espera", counts.waiting)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .child(
                        div()
                            .w(px(320.))
                            .border_r_1()
                            .border_color(gpui::rgba(theme::HAIRLINE_RGBA))
                            .flex()
                            .flex_col()
                            .p_3()
                            .child(
                                div().flex().items_center().child("[/] ").child(
                                    div()
                                        .flex_1()
                                        .border_1()
                                        .border_color(color(if self.filter_focused {
                                            theme::ACCENT
                                        } else {
                                            theme::HAIRLINE_STRONG
                                        }))
                                        .rounded(px(4.))
                                        .px_1()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|v, _, _, cx| {
                                                v.filter_focused = true;
                                                v.terminal_focused = false;
                                                cx.notify()
                                            }),
                                        )
                                        .child(if self.filter.is_empty() {
                                            "filtrar…".to_string()
                                        } else {
                                            self.filter.clone()
                                        }),
                                ),
                            )
                            .child(tree)
                            .child(div().mt_auto().text_color(color(theme::MUTE)).child(
                                "[>] trabajando  [?] te necesita  [+] en espera  [x] terminada",
                            )),
                    )
                    .child(center)
                    .child(detail),
            )
    }
}
fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    cx.new(|cx: &mut Context<MainWindow>| {
                        cx.observe_window_bounds(window, |view, window, cx| {
                            let bounds = window.bounds().size;
                            let cols = ((f32::from(bounds.width) - 580.0) / 9.0).max(2.0) as u16;
                            let rows = ((f32::from(bounds.height) - 100.0) / 18.0).max(1.0) as u16;
                            view.resize_terminal(cols, rows, cx);
                        })
                        .detach();
                        MainWindow::new()
                    })
                },
            )
            .expect("failed to open the main window");
        window.update(cx, |v, _, cx| v.start(cx)).ok();
        cx.activate(true);
    });
}
