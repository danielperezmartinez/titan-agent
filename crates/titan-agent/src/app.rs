//! Main GPUI window for titan-agent.

use crate::{
    client::{DaemonClient, Incoming, socket_from_env, spawn_daemon},
    terminal_view::{TerminalView, TerminalViewEvent},
    theme, view_model,
};
use futures::StreamExt;
use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, Context, Entity, FocusHandle, Focusable, HighlightStyle, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, StatefulInteractiveElement, Styled,
    StyledText, Window, div, px, rgb, rgba,
};
use std::path::Path;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use titan_agent_core::{
    protocol::{Event, Request, Response},
    session::{Project, Session, SessionKey},
    tree::{TreeRow, build_tree},
};

#[derive(Clone, PartialEq, Eq)]
enum Selection {
    Session(SessionKey),
    Project(Project),
    #[allow(dead_code)]
    NewSession,
}

pub struct MainWindow {
    sessions: Vec<Session>,
    selection: Option<Selection>,
    filter: String,
    filter_focus: FocusHandle,
    client: Option<DaemonClient>,
    disconnected: bool,
    errors: HashMap<u64, String>,
    open_requests: HashMap<u64, SessionKey>,
    stop_requests: HashMap<u64, SessionKey>,
    terminal_errors: HashMap<SessionKey, String>,
    stop_errors: HashMap<SessionKey, String>,
    active_terminals: HashSet<SessionKey>,
    terminals: HashMap<SessionKey, Entity<TerminalView>>,
    terminal_ids: HashMap<SessionKey, u64>,
    ended: HashMap<SessionKey, bool>,
    subscriptions: Vec<gpui::Subscription>,
    confirm_stop: Option<(SessionKey, std::time::Instant)>,
    focused_session: Option<SessionKey>,
    monitor_tab: &'static str,
    initial_selection: Option<String>,
    new_session_project: Option<Project>,
}

impl MainWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            sessions: Vec::new(),
            selection: None,
            filter: String::new(),
            filter_focus: cx.focus_handle(),
            client: None,
            disconnected: true,
            errors: HashMap::new(),
            open_requests: HashMap::new(),
            stop_requests: HashMap::new(),
            terminal_errors: HashMap::new(),
            stop_errors: HashMap::new(),
            active_terminals: HashSet::new(),
            terminals: HashMap::new(),
            terminal_ids: HashMap::new(),
            ended: HashMap::new(),
            subscriptions: Vec::new(),
            confirm_stop: None,
            focused_session: None,
            monitor_tab: "grafo",
            initial_selection: std::env::var("TITAN_AGENT_SELECT").ok(),
            new_session_project: None,
        };
        this.connect(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_spawn(async {
                    std::thread::sleep(Duration::from_secs(30));
                })
                .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        this
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        let socket = socket_from_env();
        let client_name = format!("titan-agent {}", env!("CARGO_PKG_VERSION"));
        cx.spawn(async move |this, cx| {
            loop {
                let path = socket.clone();
                let name = client_name.clone();
                let connected = cx
                    .background_spawn(async move {
                        DaemonClient::connect_or_spawn(&path, &name, || spawn_daemon(&path))
                    })
                    .await;
                match connected {
                    Ok((client, _, mut rx)) => {
                        let cloned = this.clone();
                        let _ = this.update(cx, |view, cx| {
                            view.disconnected = false;
                            view.client = Some(client);
                            view.open_requests.clear();
                            view.stop_requests.clear();
                            view.errors.clear();
                            if let Some(client) = &view.client {
                                client.send(Request::Subscribe);
                            }
                            view.terminal_ids.clear();
                            let reopen: Vec<_> = view.active_terminals.iter().cloned().collect();
                            for key in reopen {
                                view.request_terminal(key, cx);
                            }
                            cx.notify();
                        });
                        while let Some(message) = rx.next().await {
                            let disconnected = matches!(message, Incoming::Disconnected(_));
                            let _ = cloned.update(cx, |view, cx| {
                                view.incoming(message, cx);
                            });
                            if disconnected {
                                break;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = this.update(cx, |view, cx| {
                            view.disconnected = true;
                            view.client = None;
                            cx.notify();
                        });
                    }
                }
                let _ = this.update(cx, |view, cx| {
                    view.disconnected = true;
                    view.client = None;
                    cx.notify();
                });
                cx.background_spawn(async {
                    std::thread::sleep(Duration::from_secs(2));
                })
                .await;
            }
        })
        .detach();
    }

    fn select_new_session(&mut self) {
        if self.selection != Some(Selection::NewSession) {
            self.new_session_project = self
                .selected_session()
                .map(|s| s.project.clone())
                .or_else(|| match &self.selection {
                    Some(Selection::Project(p)) => Some(p.clone()),
                    _ => None,
                })
                .or_else(|| self.sessions.first().map(|s| s.project.clone()));
        }
        self.focused_session = None;
        self.selection = Some(Selection::NewSession);
    }

    fn request_terminal(&mut self, key: SessionKey, cx: &mut Context<Self>) {
        let Some(client) = &self.client else {
            return;
        };
        if self.terminal_ids.contains_key(&key)
            || self.open_requests.values().any(|pending| pending == &key)
        {
            return;
        }
        self.terminal_errors.remove(&key);
        let (cols, rows) = self
            .terminals
            .get(&key)
            .and_then(|view| view.read(cx).grid())
            .unwrap_or((100, 30));
        let id = client.send(Request::OpenTerminal {
            session: key.clone(),
            cols,
            rows,
        });
        self.open_requests.insert(id, key);
    }

    fn incoming(&mut self, message: Incoming, cx: &mut Context<Self>) {
        match message {
            Incoming::Response {
                id,
                response: Response::Done,
            } => {
                if let Some(key) = self.stop_requests.remove(&id) {
                    self.stop_errors.remove(&key);
                    self.confirm_stop = None;
                }
            }
            Incoming::Response {
                id: _,
                response: Response::Sessions { sessions },
            } => self.set_sessions(sessions),
            Incoming::Event(Event::Sessions { sessions }) => self.set_sessions(sessions),
            Incoming::Response {
                id,
                response:
                    Response::TerminalOpened {
                        terminal, session, ..
                    },
            } => {
                self.open_requests.remove(&id);
                self.terminal_ids.insert(session.clone(), terminal);
                self.ended.insert(session.clone(), false);
                self.active_terminals.insert(session.clone());
                if !self.terminals.contains_key(&session) {
                    let view = cx.new(TerminalView::new);
                    let key = session.clone();
                    let subscription =
                        cx.subscribe(&view, move |this, _, event: &TerminalViewEvent, cx| {
                            let Some(terminal) = this.terminal_ids.get(&key).copied() else {
                                return;
                            };
                            if let Some(client) = &this.client {
                                match event {
                                    TerminalViewEvent::Input(data) => {
                                        client.send(Request::Input {
                                            terminal,
                                            data: data.clone().into(),
                                        });
                                    }
                                    TerminalViewEvent::Resized { cols, rows } => {
                                        client.send(Request::Resize {
                                            terminal,
                                            cols: *cols,
                                            rows: *rows,
                                        });
                                    }
                                }
                            }
                            cx.notify();
                        });
                    self.subscriptions.push(subscription);
                    self.terminals.insert(session.clone(), view);
                }
                if self.selection == Some(Selection::Session(session.clone())) {
                    self.focused_session = None;
                }
            }
            Incoming::Event(Event::TerminalSnapshot {
                terminal,
                cols,
                rows,
                data,
            }) => {
                if let Some((key, _)) = self.terminal_ids.iter().find(|(_, id)| **id == terminal)
                    && let Some(view) = self.terminals.get(key).cloned()
                {
                    view.update(cx, |terminal, cx| {
                        terminal.load_snapshot(cols, rows, &data.0, cx)
                    });
                }
            }
            Incoming::Event(Event::TerminalOutput { terminal, data }) => {
                if let Some((key, _)) = self.terminal_ids.iter().find(|(_, id)| **id == terminal)
                    && let Some(view) = self.terminals.get(key).cloned()
                {
                    view.update(cx, |terminal, cx| terminal.feed(&data.0, cx));
                }
            }
            Incoming::Event(Event::TerminalExited { terminal, .. }) => {
                if let Some((key, _)) = self.terminal_ids.iter().find(|(_, id)| **id == terminal) {
                    self.ended.insert(key.clone(), true);
                    self.active_terminals.remove(key);
                }
            }
            Incoming::Error { id, error } => {
                if let Some(key) = self.open_requests.remove(&id) {
                    self.terminal_errors
                        .insert(key.clone(), error.message.clone());
                    self.errors.insert(id, error.message.clone());
                } else if let Some(key) = self.stop_requests.remove(&id) {
                    self.stop_errors.insert(key, error.message);
                } else {
                    self.errors.insert(id, error.message);
                }
            }
            Incoming::Disconnected(_) => {
                self.disconnected = true;
                self.client = None;
                self.open_requests.clear();
                self.stop_requests.clear();
            }
            _ => {}
        }
        cx.notify();
    }

    fn set_sessions(&mut self, sessions: Vec<Session>) {
        self.sessions = sessions;
        let selection_exists = match &self.selection {
            Some(Selection::Session(key)) => self.sessions.iter().any(|s| &s.key == key),
            Some(Selection::Project(project)) => {
                self.sessions.iter().any(|s| &s.project == project)
            }
            Some(Selection::NewSession) => true,
            None => false,
        };
        if !selection_exists {
            self.selection = None;
            self.focused_session = None;
        }
        if !selection_exists && let Some(requested) = self.initial_selection.take() {
            self.selection = match requested.as_str() {
                "new" => Some(Selection::NewSession),
                value if value.starts_with("project:") => {
                    let name = &value[8..];
                    self.sessions
                        .iter()
                        .find(|session| session.project.name == name)
                        .map(|session| Selection::Project(session.project.clone()))
                }
                value if value.starts_with("session:") => {
                    let id = &value[8..];
                    self.sessions
                        .iter()
                        .find(|session| session.key.id == id)
                        .map(|session| Selection::Session(session.key.clone()))
                }
                _ => None,
            };
        }
        if !selection_exists && self.selection.is_none() {
            self.selection = build_tree(&self.sessions, &self.filter)
                .into_iter()
                .find_map(|row| match row {
                    TreeRow::Session { key, .. } => Some(Selection::Session(key)),
                    TreeRow::Project { .. } => None,
                });
        }
        if self.selection == Some(Selection::NewSession) && self.new_session_project.is_none() {
            self.new_session_project = self.sessions.first().map(|s| s.project.clone());
        }
    }

    fn text(value: impl Into<String>, size: f32, color: u32) -> gpui::Div {
        div()
            .text_size(px(size))
            .text_color(rgb(color))
            .child(value.into())
    }

    fn color(value: u32) -> gpui::Rgba {
        if value > 0x00ff_ffff {
            rgba(value)
        } else {
            rgb(value)
        }
    }

    fn selected_session(&self) -> Option<&Session> {
        let Selection::Session(key) = self.selection.as_ref()? else {
            return None;
        };
        self.sessions.iter().find(|s| &s.key == key)
    }

    fn fallback_project(&self) -> Option<Project> {
        if let Some(project) = &self.new_session_project {
            return Some(project.clone());
        }
        if let Some(session) = self.selected_session() {
            return Some(session.project.clone());
        }
        if let Some(Selection::Project(project)) = &self.selection {
            return Some(project.clone());
        }
        self.sessions
            .last()
            .map(|session| session.project.clone())
            .or_else(|| self.sessions.first().map(|session| session.project.clone()))
    }

    fn render_monitor(&self, project: &Project, cx: &mut Context<Self>) -> gpui::Div {
        let (title, subtitle, sessions_line) = view_model::monitor_text(&self.sessions, project);
        let tabs = ["grafo", "carriles", "mesa de control"];
        let mut tab_row = div().flex().items_center().gap(px(4.0));
        for tab in tabs {
            let active = self.monitor_tab == tab;
            tab_row = tab_row.child(
                div()
                    .id(format!("monitor-tab-{tab}"))
                    .h(px(30.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(rgb(if active {
                        theme::ACCENT
                    } else {
                        theme::HAIRLINE_STRONG
                    }))
                    .bg(if active {
                        rgb(theme::SURFACE_ELEVATED)
                    } else {
                        rgb(theme::CANVAS)
                    })
                    .child(Self::text(
                        tab,
                        12.0,
                        if active { theme::INK } else { theme::MUTE },
                    ))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.monitor_tab = tab;
                            cx.notify();
                        }),
                    ),
            );
        }
        let count = self
            .sessions
            .iter()
            .filter(|session| session.project == *project)
            .count();
        let team_live = self.sessions.iter().any(|session| {
            session.project == *project
                && session.parent.as_ref().is_some_and(|parent| {
                    self.sessions
                        .iter()
                        .any(|candidate| candidate.project == *project && &candidate.key == parent)
                })
        });
        div()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .min_h(px(40.0))
                    .px(px(16.0))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .border_b_1()
                    .border_color(Self::color(theme::HAIRLINE_RGBA))
                    .child(Self::text(
                        format!("[#] {}", project.name),
                        13.0,
                        theme::INK,
                    ))
                    .child(Self::text(format!("{count} sesiones"), 12.0, theme::MUTE))
                    .when(team_live, |el| {
                        el.child(Self::text("[*] en vivo", 12.0, theme::SUCCESS))
                    })
                    .child(div().flex_1())
                    .child(tab_row),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.0))
                    .bg(rgb(theme::TERMINAL_BG))
                    .child(div().max_w(px(720.0)).text_center().child(Self::text(
                        title,
                        20.0,
                        theme::INK,
                    )))
                    .child(div().max_w(px(720.0)).text_center().child(Self::text(
                        subtitle,
                        13.0,
                        theme::MUTE,
                    )))
                    .child(div().max_w(px(720.0)).text_center().child(Self::text(
                        sessions_line,
                        13.0,
                        theme::MUTE,
                    ))),
            )
    }

    fn render_new_session(&self) -> gpui::Div {
        let project = self.fallback_project();
        let project_name = project
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("proyecto");
        let chips = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .h(px(32.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .border_1()
                    .border_color(rgb(theme::HAIRLINE_STRONG))
                    .rounded(px(4.0))
                    .child(Self::text("cl", 12.0, theme::CLAUDE))
                    .child(Self::text("Claude Code", 12.0, theme::BODY))
                    .child(Self::text("[v]", 12.0, theme::MUTE)),
            )
            .children(["opus", "high", "default"].into_iter().map(|label| {
                div()
                    .h(px(32.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .border_1()
                    .border_color(rgb(theme::HAIRLINE_STRONG))
                    .rounded(px(4.0))
                    .child(Self::text(label, 12.0, theme::BODY))
                    .child(Self::text("[v]", 12.0, theme::MUTE))
            }))
            .child(
                div()
                    .h(px(32.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .border_1()
                    .border_color(rgb(theme::HAIRLINE_STRONG))
                    .rounded(px(4.0))
                    .child(Self::text(format!("[#] {project_name}"), 12.0, theme::BODY))
                    .child(Self::text("[v]", 12.0, theme::MUTE)),
            );
        div()
            .flex_1()
            .p(px(16.0))
            .flex()
            .flex_col()
            .bg(rgb(theme::TERMINAL_BG))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .child(Self::text("Nueva sesión", 28.0, theme::INK))
                    .child(
                        div()
                            .flex()
                            .gap(px(4.0))
                            .child(Self::text("en", 13.0, theme::MUTE))
                            .child(Self::text(format!("[#] {project_name}"), 13.0, theme::BODY))
                            .child(Self::text("con", 13.0, theme::MUTE))
                            .child(Self::text("Claude Code", 13.0, theme::CLAUDE)),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(820.0))
                    .mx_auto()
                    .border_1()
                    .border_color(rgb(theme::ACCENT))
                    .rounded(px(4.0))
                    .bg(rgb(theme::CANVAS))
                    .child(div().h(px(100.0)).p(px(12.0)).child(Self::text(
                        "Escribe el encargo. titan-agent no lo completa ni lo reescribe.",
                        13.0,
                        theme::MUTE,
                    )))
                    .child(
                        div()
                            .border_t_1()
                            .border_color(Self::color(theme::HAIRLINE_RGBA))
                            .p(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(chips)
                            .child(div().flex_1())
                            .child(
                                div()
                                    .h(px(32.0))
                                    .px(px(16.0))
                                    .flex()
                                    .items_center()
                                    .border_1()
                                    .border_color(rgb(theme::ACCENT))
                                    .rounded(px(4.0))
                                    .bg(rgb(theme::SURFACE_ELEVATED))
                                    .child(Self::text("[ok] lanzar", 13.0, theme::INK)),
                            ),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(820.0))
                    .mx_auto()
                    .mt(px(8.0))
                    .px(px(4.0))
                    .flex()
                    .gap(px(8.0))
                    .child(Self::text("se ejecutará", 11.0, theme::STONE))
                    .child(Self::text("disponible en la fase 2", 11.0, theme::MUTE)),
            )
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div()
            .id("session-list")
            .flex()
            .flex_col()
            .gap(px(3.0))
            .overflow_y_scroll();
        for row in build_tree(&self.sessions, &self.filter) {
            match row {
                TreeRow::Project {
                    project,
                    count,
                    team_live,
                } => {
                    let active = self.selection == Some(Selection::Project(project.clone()));
                    let project_for_click = project.clone();
                    list = list.child(
                        div()
                            .id(format!("project-{}", project.name))
                            .min_h(px(32.0))
                            .mt(px(8.0))
                            .flex_none()
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .border_l_2()
                            .border_color(rgb(if active { theme::ACCENT } else { theme::CANVAS }))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.focused_session = None;
                                    this.selection =
                                        Some(Selection::Project(project_for_click.clone()));
                                    cx.notify();
                                }),
                            )
                            .bg(if active {
                                rgb(theme::SURFACE)
                            } else {
                                rgb(theme::CANVAS)
                            })
                            .child(Self::text("[#]", 12.0, theme::MUTE))
                            .child(
                                div()
                                    .flex_grow(1.0)
                                    .flex_shrink(1.0)
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(Self::text(
                                        project.name.clone(),
                                        12.0,
                                        if active { theme::INK } else { theme::BODY },
                                    )),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .h(px(1.0))
                                    .bg(rgb(theme::HAIRLINE_STRONG)),
                            )
                            .child(
                                Self::text(
                                    if team_live { "[*]" } else { "" },
                                    12.0,
                                    theme::SUCCESS,
                                )
                                .flex_none(),
                            )
                            .child(Self::text(count.to_string(), 12.0, theme::MUTE).flex_none()),
                    );
                }
                TreeRow::Session { key, depth, branch } => {
                    let row = self.sessions.iter().find(|s| s.key == key);
                    if let Some(session) = row {
                        let (marker, _, color) = view_model::state_style(session.state);
                        let active =
                            self.selection.as_ref() == Some(&Selection::Session(key.clone()));
                        let (tag, model) = view_model::pill(session);
                        let key_for_click = key.clone();
                        list = list.child(
                            div()
                                .id(format!("session-{}", key.id))
                                .min_h(px(36.0))
                                .flex_none()
                                .pl(px(if depth == 0 {
                                    12.0
                                } else {
                                    20.0 + 12.0 * (depth as f32 - 1.0)
                                }))
                                .pr(px(12.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .border_l_2()
                                .border_color(rgb(if active {
                                    theme::ACCENT
                                } else {
                                    theme::CANVAS
                                }))
                                .hover(|style| style.bg(rgb(theme::SURFACE)))
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.focused_session = None;
                                        this.selection =
                                            Some(Selection::Session(key_for_click.clone()));
                                        cx.notify();
                                    }),
                                )
                                .bg(if active {
                                    rgb(theme::SURFACE)
                                } else {
                                    rgb(theme::CANVAS)
                                })
                                .child(
                                    Self::text(
                                        if depth == 0 { "" } else { branch.marker() },
                                        13.0,
                                        theme::STONE,
                                    )
                                    .flex_none(),
                                )
                                .child(Self::text(marker, 13.0, color).flex_none())
                                .child(
                                    div()
                                        .flex_grow(1.0)
                                        .flex_shrink(1.0)
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(Self::text(
                                            session.name.clone(),
                                            13.0,
                                            if session.state.is_live() {
                                                theme::BODY
                                            } else {
                                                theme::MUTE
                                            },
                                        )),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .px(px(4.0))
                                        .h(px(18.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .border_1()
                                        .border_color(rgb(view_model::agent_color(key.cli)))
                                        .rounded(px(4.0))
                                        .child(Self::text(
                                            tag,
                                            11.0,
                                            view_model::agent_color(key.cli),
                                        ))
                                        .child(Self::text(model, 11.0, theme::MUTE)),
                                ),
                        );
                    }
                }
            }
        }
        let mut filter = div()
            .id("session-filter")
            .h(px(32.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .bg(rgb(theme::SURFACE))
            .rounded(px(4.0))
            .child(Self::text("[/]", 12.0, theme::MUTE));
        filter = filter.child(Self::text(
            if self.filter.is_empty() {
                "filtrar por nombre, agente o proyecto"
            } else {
                &self.filter
            },
            12.0,
            if self.filter.is_empty() {
                theme::MUTE
            } else {
                theme::BODY
            },
        ));
        filter = filter.track_focus(&self.filter_focus).on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                window.focus(&this.filter_focus, cx);
                cx.notify();
            }),
        );
        let new_active = self.selection == Some(Selection::NewSession);
        div()
            .w(px(300.0))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(theme::HAIRLINE_STRONG))
            .child(
                div()
                    .p(px(12.0))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("new-session")
                            .h(px(36.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .bg(rgb(if new_active {
                                theme::SURFACE
                            } else {
                                theme::SURFACE_ELEVATED
                            }))
                            .border_1()
                            .border_color(rgb(if new_active {
                                theme::ACCENT
                            } else {
                                theme::HAIRLINE_STRONG
                            }))
                            .rounded(px(4.0))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.select_new_session();
                                    cx.notify();
                                }),
                            )
                            .child(Self::text("[+]", 13.0, theme::INK))
                            .child(Self::text("Nueva sesión", 13.0, theme::INK))
                            .child(div().flex_1())
                            .child(Self::text("ctrl+n", 11.0, theme::MUTE)),
                    )
                    .child(
                        filter.on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                            let key = &event.keystroke.key;
                            if key == "backspace" {
                                this.filter.pop();
                            } else if key == "escape" {
                                this.filter.clear();
                            } else if !event.keystroke.modifiers.control
                                && !event.keystroke.modifiers.platform
                                && let Some(ch) = event.keystroke.key_char.as_ref()
                            {
                                this.filter.push_str(ch);
                            }
                            cx.notify();
                        })),
                    )
                    .child(
                        div()
                            .pt(px(4.0))
                            .flex()
                            .justify_between()
                            .child(Self::text("SESIONES", 12.0, theme::INK))
                            .child(Self::text("por proyecto [v]", 12.0, theme::MUTE)),
                    ),
            )
            .child(list.flex_1().min_h(px(0.0)).pb(px(12.0)))
            .child(
                div()
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(theme::HAIRLINE_STRONG))
                    .p(px(12.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .child(Self::text("cl Claude Code", 11.0, theme::MUTE))
                    .child(Self::text("cx Codex", 11.0, theme::MUTE))
                    .child(Self::text("[>] trabajando", 11.0, theme::MUTE))
                    .child(Self::text("[?] te necesita", 11.0, theme::MUTE))
                    .child(Self::text("[+] en espera", 11.0, theme::MUTE))
                    .child(Self::text("[x] terminada", 11.0, theme::MUTE)),
            )
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = div()
            .h(px(48.0))
            .flex_none()
            .px(px(16.0))
            .flex()
            .items_center()
            .gap(px(16.0))
            .border_b_1()
            .border_color(rgb(theme::HAIRLINE_STRONG))
            .child(Self::text("titan-agent", 16.0, theme::INK))
            .child(Self::text("torre de control", 12.0, theme::MUTE))
            .child(div().flex_1())
            .when(self.disconnected, |el| {
                el.child(Self::text(
                    "[x] sin conexión con titan-agentd",
                    12.0,
                    theme::DANGER,
                ))
            })
            .child(div().flex().gap(px(12.0)).children(
                view_model::header_counts(&self.sessions).into_iter().map(
                    |(marker, label, color)| {
                        div()
                            .flex()
                            .gap(px(4.0))
                            .child(Self::text(marker, 12.0, color))
                            .child(Self::text(label, 12.0, theme::MUTE))
                    },
                ),
            ))
            .child(Self::text("[i]", 14.0, theme::BODY))
            .child(Self::text("[*]", 14.0, theme::BODY));
        let center = if let Some(session) = self.selected_session().cloned() {
            if self.focused_session.as_ref() != Some(&session.key)
                && let Some(view) = self.terminals.get(&session.key)
            {
                self.focused_session = Some(session.key.clone());
                _window.focus(&view.read(cx).focus_handle(cx), cx);
            }
            let (marker, state, state_color) = view_model::state_style(session.state);
            if session.attach.is_some()
                && !self.terminal_ids.contains_key(&session.key)
                && !self.terminal_errors.contains_key(&session.key)
            {
                let key = session.key.clone();
                self.request_terminal(key, cx);
            }
            let info = div()
                .h(px(40.0))
                .flex_none()
                .px(px(16.0))
                .flex()
                .items_center()
                .gap(px(12.0))
                .border_b_1()
                .border_color(rgb(theme::HAIRLINE_STRONG))
                .child(Self::text(session.name.clone(), 13.0, theme::INK).flex_none())
                .child(Self::text(format!("{marker} {state}"), 12.0, state_color).flex_none())
                .child(
                    Self::text(
                        view_model::agent_line(&session),
                        12.0,
                        view_model::agent_color(session.key.cli),
                    )
                    .flex_none(),
                )
                .child(
                    div()
                        .flex_grow(1.0)
                        .flex_shrink(1.0)
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(Self::text(
                            titan_agent_core::discovery::display_path(
                                &session.cwd,
                                std::env::var("HOME").ok().as_deref(),
                            ),
                            12.0,
                            theme::MUTE,
                        )),
                )
                .child(div().flex_1().min_w(px(0.0)))
                .child(
                    Self::text(
                        view_model::attach_line(&session).unwrap_or_default(),
                        12.0,
                        theme::STONE,
                    )
                    .flex_none(),
                );
            let view = self.terminals.get(&session.key).cloned();
            let has_terminal_view = view.is_some();
            let terminal_key = session.key.clone();
            let body = if session.attach.is_none() {
                let (title, message) = view_model::not_attachable_text(session.key.cli);
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .bg(rgb(theme::TERMINAL_BG))
                    .child(Self::text(title, 20.0, theme::INK))
                    .child(div().max_w(px(560.0)).text_center().child(Self::text(
                        message,
                        13.0,
                        theme::MUTE,
                    )))
            } else {
                let reopening = self.open_requests.values().any(|key| key == &terminal_key);
                let terminal = if self.terminal_errors.contains_key(&terminal_key) {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Self::text(
                            self.terminal_errors[&terminal_key].clone(),
                            13.0,
                            theme::DANGER,
                        ))
                } else if reopening {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Self::text("[>] reabriendo terminal…", 12.0, theme::MUTE))
                } else if self.ended.get(&terminal_key).copied().unwrap_or(false) {
                    let key = terminal_key.clone();
                    div().flex_1().flex().items_center().justify_center().child(
                        div()
                            .flex()
                            .gap(px(5.0))
                            .child(Self::text("[x] el terminal terminó ·", 12.0, theme::MUTE))
                            .child(
                                Self::text("[>] reabrir", 12.0, theme::ACCENT).on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.terminal_ids.remove(&key);
                                        this.terminal_errors.remove(&key);
                                        this.open_requests.retain(|_, pending| pending != &key);
                                        this.ended.insert(key.clone(), true);
                                        this.request_terminal(key.clone(), cx);
                                        cx.notify();
                                    }),
                                ),
                            ),
                    )
                } else if let Some(view) = view {
                    div().flex_1().child(view)
                } else {
                    let message = self
                        .open_requests
                        .iter()
                        .find(|(_, key)| *key == &terminal_key)
                        .and_then(|(id, _)| self.errors.get(id))
                        .cloned()
                        .unwrap_or_default();
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Self::text(message, 13.0, theme::DANGER))
                };
                let terminal_focus = has_terminal_view;
                let entry_key = terminal_key.clone();
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .bg(rgb(theme::TERMINAL_BG))
                    .child(terminal)
                    .child(
                        div()
                            .mx(px(16.0))
                            .mb(px(16.0))
                            .p(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .border_1()
                            .border_color(rgb(if terminal_focus {
                                theme::ACCENT
                            } else {
                                theme::HAIRLINE_STRONG
                            }))
                            .rounded(px(4.0))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    if let Some(view) = this.terminals.get(&entry_key) {
                                        window.focus(&view.read(cx).focus_handle(cx), cx)
                                    }
                                }),
                            )
                            .child(Self::text(">", 13.0, theme::MUTE))
                            .child(Self::text(
                                "escribe aquí: hablas directamente con el CLI oficial",
                                13.0,
                                theme::MUTE,
                            ))
                            .child(div().w(px(8.0)).h(px(16.0)).bg(rgb(theme::ACCENT))),
                    )
            };
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let detail = view_model::detail(
                &self.sessions,
                &session.key,
                now_ms,
                std::env::var("HOME").ok().as_deref(),
            );
            let detail_panel = div()
                .w(px(260.0))
                .flex_none()
                .min_h(px(0.0))
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(16.0))
                .border_l_1()
                .border_color(rgb(theme::HAIRLINE_STRONG))
                .text_size(px(12.0));
            let detail_panel = if let Some(detail) = detail {
                let key_project = session.project.clone();
                let mut talk_cards = div()
                    .id("detail-talks")
                    .flex_initial()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(8.0));
                if detail.talks.is_empty() {
                    talk_cards = talk_cards.child(Self::text(
                        "sin comunicaciones conocidas",
                        12.0,
                        theme::MUTE,
                    ));
                } else {
                    for talk in detail.talks {
                        talk_cards = talk_cards.child(
                            div()
                                .w_full()
                                .min_w(px(0.0))
                                .p(px(8.0))
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .bg(rgb(theme::SURFACE))
                                .rounded(px(4.0))
                                .child({
                                    let heading = format!("{} {}", talk.direction, talk.who);
                                    let who_start = talk.direction.len() + 1;
                                    let who_end = heading.len();
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(theme::MUTE))
                                        .child(StyledText::new(heading).with_highlights([(
                                            who_start..who_end,
                                            HighlightStyle::color(rgb(talk.color).into()),
                                        )]))
                                })
                                .child(
                                    Self::text(talk.what, 12.0, theme::MUTE).whitespace_normal(),
                                ),
                        );
                    }
                }
                let top = div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(Self::text("DETALLE", 12.0, theme::MUTE))
                    .child(
                        div()
                            .grid()
                            .grid_cols(2)
                            .gap(px(8.0))
                            .child(Self::text("proyecto", 12.0, theme::MUTE))
                            .child(Self::text(detail.project.clone(), 12.0, theme::BODY))
                            .child(Self::text("rol", 12.0, theme::MUTE))
                            .child(Self::text(detail.role, 12.0, theme::BODY))
                            .child(Self::text("lanzada por", 12.0, theme::MUTE))
                            .child(Self::text(detail.launched_by, 12.0, theme::BODY))
                            .child(Self::text("estado desde", 12.0, theme::MUTE))
                            .child(Self::text(detail.since, 12.0, theme::BODY)),
                    )
                    .child(Self::text("HABLA CON", 12.0, theme::MUTE));
                detail_panel.child(top).child(talk_cards).child(
                    Self::text("[>] ver el monitor del proyecto", 12.0, theme::ACCENT)
                        .flex_none()
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.focused_session = None;
                                this.selection = Some(Selection::Project(key_project.clone()));
                                cx.notify();
                            }),
                        ),
                )
            } else {
                detail_panel.child(Self::text("DETALLE", 12.0, theme::MUTE).flex_none())
            };
            let open_path = Path::new(&session.cwd).to_path_buf();
            let stopped_key = session.key.clone();
            let action =
                view_model::stop_action(&session, self.terminal_ids.contains_key(&session.key));
            let stop_text = if self
                .confirm_stop
                .as_ref()
                .is_some_and(|(k, t)| k == &session.key && t.elapsed() < Duration::from_secs(4))
            {
                "[x] confirmar"
            } else {
                match action {
                    view_model::StopAction::StopSession => "[x] detener",
                    view_model::StopAction::CloseTerminal => "[x] cerrar terminal",
                    view_model::StopAction::Disabled => "[x] detener",
                }
            };
            let stop_error = self.stop_errors.get(&session.key).cloned();
            let bottom = div()
                .mt_auto()
                .flex_none()
                .flex()
                .flex_wrap()
                .gap(px(8.0))
                .child(
                    div()
                        .h(px(32.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .border_1()
                        .border_color(rgb(theme::HAIRLINE_STRONG))
                        .rounded(px(4.0))
                        .child(Self::text("[d] abrir carpeta", 12.0, theme::BODY))
                        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                            cx.open_with_system(&open_path);
                        }),
                )
                .child(
                    div()
                        .h(px(32.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .border_1()
                        .border_color(rgb(if matches!(action, view_model::StopAction::Disabled) {
                            theme::HAIRLINE_STRONG
                        } else {
                            theme::DANGER
                        }))
                        .rounded(px(4.0))
                        .child(Self::text(
                            stop_text,
                            12.0,
                            if matches!(action, view_model::StopAction::Disabled) {
                                theme::STONE
                            } else {
                                theme::DANGER
                            },
                        ))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                if matches!(action, view_model::StopAction::Disabled) {
                                    return;
                                }
                                if this.confirm_stop.as_ref().is_some_and(|(k, t)| {
                                    k == &stopped_key && t.elapsed() < Duration::from_secs(4)
                                }) {
                                    this.confirm_stop = None;
                                    this.stop_errors.remove(&stopped_key);
                                    if let Some(client) = &this.client {
                                        match action {
                                            view_model::StopAction::StopSession => {
                                                let id = client.send(Request::StopSession {
                                                    session: stopped_key.clone(),
                                                });
                                                this.stop_requests.insert(id, stopped_key.clone());
                                            }
                                            view_model::StopAction::CloseTerminal => {
                                                if let Some(id) =
                                                    this.terminal_ids.get(&stopped_key)
                                                {
                                                    let request =
                                                        client.send(Request::CloseTerminal {
                                                            terminal: *id,
                                                        });
                                                    this.stop_requests
                                                        .insert(request, stopped_key.clone());
                                                }
                                            }
                                            view_model::StopAction::Disabled => {}
                                        }
                                    }
                                } else {
                                    this.confirm_stop =
                                        Some((stopped_key.clone(), std::time::Instant::now()));
                                    let key = stopped_key.clone();
                                    cx.spawn(async move |this, cx| {
                                        cx.background_spawn(async {
                                            std::thread::sleep(Duration::from_secs(4));
                                        })
                                        .await;
                                        let _ = this.update(cx, |view, cx| {
                                            if view.confirm_stop.as_ref().is_some_and(
                                                |(current, started)| {
                                                    current == &key
                                                        && started.elapsed()
                                                            >= Duration::from_secs(4)
                                                },
                                            ) {
                                                view.confirm_stop = None;
                                                cx.notify();
                                            }
                                        });
                                    })
                                    .detach();
                                }
                                cx.notify();
                            }),
                        ),
                );
            let detail_panel = detail_panel.child(div().flex_1());
            let detail_panel = if let Some(message) = stop_error {
                detail_panel
                    .child(Self::text(message, 12.0, theme::DANGER))
                    .child(bottom)
            } else {
                detail_panel.child(bottom)
            };
            div()
                .flex_1()
                .min_h(px(0.0))
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(info)
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .min_w(px(0.0))
                        .flex()
                        .child(body)
                        .child(detail_panel),
                )
        } else if let Some(Selection::Project(project)) = self.selection.clone() {
            self.render_monitor(&project, cx)
        } else {
            self.render_new_session()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::CANVAS))
            .text_color(rgb(theme::BODY))
            .child(header)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .flex()
                    .child(self.sidebar(cx))
                    .child(center.flex_1().min_h(px(0.0)).min_w(px(0.0))),
            )
            .font_family(theme::FONT_FAMILY)
            .text_color(rgb(theme::BODY))
            .text_size(px(14.0))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key.eq_ignore_ascii_case("n")
                    && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
                {
                    this.select_new_session();
                    cx.notify();
                }
            }))
    }
}
