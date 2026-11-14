use crate::{
    client::{ClientError, Connection, connect_or_spawn, daemon_program, default_socket_path},
    keymap::key_from_gpui,
    terminal_model::TerminalModel,
    theme,
};
use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, KeyDownEvent, MouseButton,
    StatefulInteractiveElement, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb,
    size,
};
use gpui_platform::application;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::Duration;
use titan_agent_core::{
    keys::{Modifiers, encode_key, encode_paste},
    protocol::StopAction,
    protocol::{ClientRequest, DaemonNotification, Session, SessionsResult},
    session::{SessionState, StateCounts},
    tree::build_tree,
};

pub(crate) struct MainWindow {
    status: String,
    sessions: Vec<Session>,
    connection: Option<Connection>,
    selected: Option<String>,
    filter: String,
    root_focus: FocusHandle,
    filter_focus: FocusHandle,
    terminal_focus: FocusHandle,
    worker: Option<mpsc::Sender<WorkItem>>,
    generation: u64,
    terminal_generation: u64,
    notification_generation: Arc<AtomicU64>,
    open_pending: bool,
    open_queued: bool,
    terminal_size: Option<(u16, u16)>,
    terminal: Option<TerminalModel>,
    terminal_session: Option<String>,
    terminal_pending: Vec<(u64, Vec<u8>)>,
    terminal_pending_exit: Option<(u64, Option<i32>)>,
    terminal_output_after: u64,
    exited: Option<Option<i32>>,
    stop_error: Option<String>,
}
struct WorkItem {
    generation: u64,
    request: ClientRequest,
}
struct WorkResult {
    generation: u64,
    notification_cutoff: u64,
    request: ClientRequest,
    result: Result<serde_json::Value, ClientError>,
}
impl MainWindow {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            status: "conectando con titan-agentd…".into(),
            sessions: vec![],
            connection: None,
            selected: None,
            filter: String::new(),
            root_focus: cx.focus_handle(),
            filter_focus: cx.focus_handle(),
            terminal_focus: cx.focus_handle(),
            worker: None,
            generation: 0,
            terminal_generation: 0,
            notification_generation: Arc::new(AtomicU64::new(0)),
            open_pending: false,
            open_queued: false,
            terminal_size: None,
            terminal: None,
            terminal_session: None,
            terminal_pending: Vec::new(),
            terminal_pending_exit: None,
            terminal_output_after: 0,
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
                    view.install_worker(conn.clone(), cx);
                    view.status.clear();
                    if let Some(notices) = conn.take_notification_events() {
                        let notices = std::sync::Arc::new(std::sync::Mutex::new(notices));
                        let notification_generation = view.notification_generation.clone();
                        cx.spawn(async move |this, cx| loop {
                            let receiver = notices.clone();
                            let notice = cx.background_executor().spawn(async move {
                                receiver.lock().unwrap().recv().ok()
                            }).await;
                            let Some((sequence, notice)) = notice else {
                                let _ = this.update(cx, |view, cx| {
                                    view.connection = None;
                                    view.worker = None;
                                    view.sessions.clear();
                                    view.selected = None;
                                    view.terminal = None;
                                    view.terminal_session = None;
                                    view.terminal_pending.clear();
                                    view.terminal_pending_exit = None;
                                    view.status = "no se pudo conectar con titan-agentd: conexión cerrada".into();
                                    cx.notify();
                                });
                                break;
                            };
                            let generation = notification_generation.load(Ordering::Acquire);
                            let _ = this.update(cx, |view, cx| view.on_notification(notice, sequence, generation, cx));
                        }).detach();
                    }
                    cx.notify();
                }
                Err(error) => {
                    view.status = format!("no se pudo conectar con titan-agentd: {}", error_text(&error));
                    cx.notify();
                }
            });
        }).detach();
    }
    fn install_worker(&mut self, connection: Connection, cx: &mut Context<Self>) {
        let (queue_tx, queue_rx) = mpsc::channel::<WorkItem>();
        let (result_tx, result_rx) = mpsc::channel::<WorkResult>();
        std::thread::spawn(move || {
            while let Ok(item) = queue_rx.recv() {
                let notification_cutoff =
                    if matches!(item.request, ClientRequest::TerminalOpen { .. }) {
                        connection.notification_sequence()
                    } else {
                        0
                    };
                let result = connection.request(item.request.clone());
                if result_tx
                    .send(WorkResult {
                        generation: item.generation,
                        notification_cutoff,
                        request: item.request,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.worker = Some(queue_tx);
        let receiver = std::sync::Arc::new(std::sync::Mutex::new(result_rx));
        cx.spawn(async move |this, cx| {
            loop {
                let receiver = receiver.clone();
                let event = cx
                    .background_executor()
                    .spawn(async move { receiver.lock().unwrap().recv().ok() })
                    .await;
                let Some(event) = event else {
                    break;
                };
                let _ = this.update(cx, |view, cx| view.on_worker_result(event, cx));
            }
        })
        .detach();
    }
    fn enqueue(&self, request: ClientRequest, generation: u64) {
        if let Some(worker) = &self.worker {
            let _ = worker.send(WorkItem {
                generation,
                request,
            });
        }
    }
    fn on_worker_result(&mut self, event: WorkResult, cx: &mut Context<Self>) {
        match event.request {
            ClientRequest::TerminalOpen {
                session,
                cols,
                rows,
            } => {
                if event.generation != self.generation
                    || self.terminal_session.as_deref() != Some(&session)
                {
                    return;
                }
                self.open_pending = false;
                self.open_queued = false;
                match event.result {
                    Ok(_) => {
                        let (target_cols, target_rows) = self.terminal_size.unwrap_or((cols, rows));
                        let mut terminal = TerminalModel::new(target_cols, target_rows);
                        self.terminal_output_after = event.notification_cutoff;
                        for (sequence, bytes) in self.terminal_pending.drain(..) {
                            if sequence > event.notification_cutoff {
                                terminal.feed(&bytes);
                            }
                        }
                        self.terminal = Some(terminal);
                        self.exited = self
                            .terminal_pending_exit
                            .take()
                            .filter(|(sequence, _)| *sequence > event.notification_cutoff)
                            .map(|(_, code)| code);
                    }
                    Err(error) => {
                        self.stop_error = Some(error_text(&error));
                        self.terminal_session = None;
                        self.terminal_pending.clear();
                        self.terminal_pending_exit = None;
                    }
                }
            }
            ClientRequest::SessionStop { .. } if event.generation == self.generation => {
                self.stop_error = event.result.err().map(|e| error_text(&e))
            }
            _ => {}
        }
        cx.notify();
    }
    fn on_notification(
        &mut self,
        n: DaemonNotification,
        sequence: u64,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
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
                if generation == self.generation
                    && self.terminal_generation == self.generation
                    && self.terminal_session.as_deref() == Some(&session)
                    && sequence > self.terminal_output_after
                {
                    if let Some(t) = &mut self.terminal {
                        t.feed(&data);
                    } else if self.open_pending {
                        self.terminal_pending.push((sequence, data));
                    }
                }
            }
            DaemonNotification::TerminalExited { session, code } => {
                if generation == self.generation
                    && self.terminal_generation == self.generation
                    && self.terminal_session.as_deref() == Some(&session)
                {
                    if self.open_pending {
                        self.terminal_pending_exit = Some((sequence, code));
                    } else if sequence > self.terminal_output_after {
                        self.exited = Some(code);
                    }
                }
            }
        }
        cx.notify()
    }
    fn close_terminal(&mut self, _cx: &mut Context<Self>) {
        if let Some(id) = self.terminal_session.take() {
            self.enqueue(
                ClientRequest::TerminalClose { session: id },
                self.generation,
            );
        }
        self.generation = self.generation.wrapping_add(1);
        self.notification_generation
            .store(self.generation, Ordering::Release);
        self.open_pending = false;
        self.open_queued = false;
        self.terminal = None;
        self.terminal_pending.clear();
        self.terminal_pending_exit = None;
        self.exited = None;
    }
    fn select(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(&id) {
            return;
        }
        self.close_terminal(cx);
        self.selected = Some(id.clone());
        self.stop_error = None;
        let Some(session) = self.sessions.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        if session.attach.is_none() {
            cx.notify();
            return;
        }
        if self.worker.is_none() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.notification_generation
            .store(self.generation, Ordering::Release);
        let generation = self.generation;
        self.terminal_session = Some(id.clone());
        self.terminal_generation = generation;
        self.terminal_pending.clear();
        self.terminal_pending_exit = None;
        self.terminal_output_after = self
            .connection
            .as_ref()
            .map_or(0, Connection::notification_sequence);
        self.terminal_size = None;
        self.open_pending = true;
        self.open_queued = false;
        self.exited = None;
        cx.notify();
    }
    pub(crate) fn resize_terminal(&mut self, cols: u16, rows: u16, cx: &mut Context<Self>) {
        let size = (cols, rows);
        if self.terminal_size == Some(size) {
            return;
        }
        self.terminal_size = Some(size);
        if let Some(model) = &mut self.terminal {
            model.resize(size.0, size.1);
        }
        if self.open_pending && !self.open_queued {
            if size.0 == 0 || size.1 == 0 {
                return;
            }
            self.open_queued = true;
            if let Some(session) = self.terminal_session.clone() {
                self.enqueue(
                    ClientRequest::TerminalOpen {
                        session,
                        cols: size.0,
                        rows: size.1,
                    },
                    self.generation,
                );
            }
        } else if (self.open_pending || self.terminal.is_some())
            && let Some(session) = self.terminal_session.clone()
        {
            self.enqueue(
                ClientRequest::TerminalResize {
                    session,
                    cols: size.0,
                    rows: size.1,
                },
                self.generation,
            );
        }
        cx.notify();
    }
    fn request_input(&self, data: Vec<u8>) {
        if data.is_empty() {
            return;
        }
        if let Some(id) = self.terminal_session.clone() {
            self.enqueue(
                ClientRequest::TerminalInput { session: id, data },
                self.generation,
            );
        }
    }
    fn on_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        let mods = k.modifiers;
        if mods.control && k.key.eq_ignore_ascii_case("q") {
            cx.quit();
            return;
        }
        if self.filter_focus.is_focused(window) {
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
        if mods.platform || !self.terminal_focus.is_focused(window) {
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
                self.request_input(encode_paste(&text, modes));
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
                self.request_input(data)
            }
        }
    }
    fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.selected.clone() else {
            return;
        };
        self.enqueue(ClientRequest::SessionStop { session }, self.generation);
        self.stop_error = None;
        cx.notify();
    }
    fn reopen(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected.clone() {
            self.close_terminal(cx);
            self.selected = None;
            self.select(id, cx);
        }
    }
}
fn relative_start(started_at: Option<i64>) -> String {
    let Some(started_at) = started_at else {
        return "—".into();
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let elapsed = now_ms.saturating_sub(started_at).max(0) as u64 / 1000;
    if elapsed < 60 {
        "hace <1 min".into()
    } else if elapsed < 3600 {
        format!("hace {} min", elapsed / 60)
    } else if elapsed < 86400 {
        format!("hace {} h", elapsed / 3600)
    } else {
        format!("hace {} d", elapsed / 86400)
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                let pill = crate::views::pill::agent_pill(s.cli, s.model.as_deref());

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
                            v.select(id.clone(), cx);
                            window.focus(&v.root_focus, cx);
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
                        .child(div().child(pill)),
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
                    .child(crate::views::pill::agent_pill(s.cli, s.model.as_deref()))
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
            } else if s.attach.is_some() {
                let (cell_lines, cursor) = self
                    .terminal
                    .as_ref()
                    .map(|t| (t.cell_lines(), t.cursor()))
                    .unwrap_or_else(|| (Vec::new(), None));
                let mut term = div()
                    .id("terminal")
                    .flex_1()
                    .overflow_hidden()
                    .bg(color(theme::TERMINAL_BG))
                    .p_2()
                    .font_family("JetBrains Mono")
                    .text_size(px(14.))
                    .line_height(px(18.))
                    .track_focus(&self.terminal_focus)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|v, _, window, cx| {
                            window.focus(&v.terminal_focus, cx);
                        }),
                    );
                let owner = cx.entity().downgrade();
                term = term.child(crate::views::terminal::terminal_canvas(
                    cell_lines, cursor, owner,
                ));
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
                ("desde", relative_start(s.started_at)),
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
            .id("window-root")
            .track_focus(&self.root_focus)
            .on_key_down(cx.listener(|v, e, window, cx| v.on_key(e, window, cx)))
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
                        div()
                            .text_color(color(theme::INK))
                            .child("titan-agent  ")
                            .child(
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
                                        .border_color(color(
                                            if self.filter_focus.is_focused(window) {
                                                theme::ACCENT
                                            } else {
                                                theme::HAIRLINE_STRONG
                                            },
                                        ))
                                        .rounded(px(4.))
                                        .px_1()
                                        .id("session-filter")
                                        .track_focus(&self.filter_focus)
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|v, _, window, cx| {
                                                window.focus(&v.filter_focus, cx);
                                                cx.notify();
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
impl Focusable for MainWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.root_focus.clone()
    }
}

pub fn run() {
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
                        let view = MainWindow::new(cx);
                        window.focus(&view.root_focus, cx);
                        view
                    })
                },
            )
            .expect("failed to open the main window");
        window.update(cx, |v, _, cx| v.start(cx)).ok();
        cx.activate(true);
    });
}
