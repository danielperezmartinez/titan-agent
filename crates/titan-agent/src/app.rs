//! Main GPUI window for titan-agent.

use crate::{
    client::{DaemonClient, Incoming, socket_from_env, spawn_daemon},
    terminal_view::TerminalView,
    theme, view_model,
};
use futures::StreamExt;
use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, px, rgb,
};
use std::{collections::HashMap, time::Duration};
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
    client: Option<DaemonClient>,
    disconnected: bool,
    errors: HashMap<u64, String>,
    open_requests: HashMap<u64, SessionKey>,
    terminals: HashMap<SessionKey, Entity<TerminalView>>,
    terminal_ids: HashMap<SessionKey, u64>,
    ended: HashMap<SessionKey, bool>,
}

impl MainWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            sessions: Vec::new(),
            selection: None,
            filter: String::new(),
            client: None,
            disconnected: true,
            errors: HashMap::new(),
            open_requests: HashMap::new(),
            terminals: HashMap::new(),
            terminal_ids: HashMap::new(),
            ended: HashMap::new(),
        };
        this.connect(cx);
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
                            if let Some(client) = &view.client {
                                client.send(Request::Subscribe);
                            }
                            let reopen: Vec<_> = view.terminals.keys().cloned().collect();
                            for key in reopen {
                                view.request_terminal(key);
                            }
                            cx.notify();
                        });
                        while let Some(message) = rx.next().await {
                            let _ = cloned.update(cx, |view, cx| {
                                view.incoming(message, cx);
                            });
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

    fn request_terminal(&mut self, key: SessionKey) {
        let Some(client) = &self.client else {
            return;
        };
        if self.terminals.contains_key(&key) {
            return;
        }
        let id = client.send(Request::OpenTerminal {
            session: key.clone(),
            cols: 100,
            rows: 30,
        });
        self.open_requests.insert(id, key);
    }

    fn incoming(&mut self, message: Incoming, cx: &mut Context<Self>) {
        match message {
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
                self.terminals
                    .entry(session)
                    .or_insert_with(|| cx.new(TerminalView::new));
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
                }
            }
            Incoming::Error { id, error } => {
                self.errors.insert(id, error.message);
            }
            Incoming::Disconnected(_) => {
                self.disconnected = true;
                self.client = None;
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
            self.selection = self
                .sessions
                .first()
                .map(|s| Selection::Session(s.key.clone()));
        }
    }

    fn text(value: impl Into<String>, size: f32, color: u32) -> gpui::Div {
        div()
            .text_size(px(size))
            .text_color(rgb(color))
            .child(value.into())
    }

    fn selected_session(&self) -> Option<&Session> {
        let Selection::Session(key) = self.selection.as_ref()? else {
            return None;
        };
        self.sessions.iter().find(|s| &s.key == key)
    }

    fn sidebar(&self) -> impl IntoElement {
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
                    list = list.child(
                        div()
                            .h(px(32.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .bg(if active {
                                rgb(theme::SURFACE)
                            } else {
                                rgb(theme::CANVAS)
                            })
                            .child(Self::text("[#]", 12.0, theme::MUTE))
                            .child(Self::text(
                                project.name.clone(),
                                12.0,
                                if active { theme::INK } else { theme::BODY },
                            ))
                            .child(Self::text("────────", 12.0, theme::STONE))
                            .child(Self::text(
                                if team_live { "[*]" } else { "" },
                                12.0,
                                theme::SUCCESS,
                            ))
                            .child(Self::text(count.to_string(), 12.0, theme::MUTE)),
                    );
                }
                TreeRow::Session { key, depth, branch } => {
                    let row = self.sessions.iter().find(|s| s.key == key);
                    if let Some(session) = row {
                        let (marker, _, color) = view_model::state_style(session.state);
                        let active =
                            self.selection.as_ref() == Some(&Selection::Session(key.clone()));
                        let (tag, model) = view_model::pill(session);
                        list = list.child(
                            div()
                                .h(px(36.0))
                                .pl(px(if depth == 0 {
                                    12.0
                                } else {
                                    20.0 + 12.0 * (depth as f32 - 1.0)
                                }))
                                .pr(px(12.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .bg(if active {
                                    rgb(theme::SURFACE)
                                } else {
                                    rgb(theme::CANVAS)
                                })
                                .child(Self::text(branch.marker(), 13.0, theme::STONE))
                                .child(Self::text(marker, 13.0, color))
                                .child(Self::text(
                                    session.name.clone(),
                                    13.0,
                                    if session.state.is_live() {
                                        theme::BODY
                                    } else {
                                        theme::MUTE
                                    },
                                ))
                                .child(Self::text(
                                    format!("{tag} {model}"),
                                    11.0,
                                    view_model::agent_color(key.cli),
                                )),
                        );
                    }
                }
            }
        }
        div().w(px(300.0)).h_full().flex().flex_col().border_r_1().border_color(rgb(theme::HAIRLINE_STRONG)).child(div().p(px(12.0)).flex().flex_col().gap(px(8.0)).child(Self::text("[+]  Nueva sesión                 ctrl+n", 13.0, theme::INK)).child(Self::text("[/]  filtrar por nombre, agente o proyecto", 12.0, theme::MUTE)).child(Self::text("SESIONES                         por proyecto [v]", 12.0, theme::INK))).child(list.flex_1()).child(Self::text("cl Claude Code   cx Codex\n[>] trabajando  [?] te necesita\n[+] en espera  [x] terminada", 11.0, theme::MUTE).p(px(12.0)))
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let header = div()
            .h(px(48.0))
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
            .child(Self::text(
                "[>] 0 trabajando   [?] 0 te necesita   [+] 0 en espera",
                12.0,
                theme::MUTE,
            ))
            .child(Self::text("[i]", 14.0, theme::BODY))
            .child(Self::text("[*]", 14.0, theme::BODY));
        let center = if let Some(session) = self.selected_session() {
            let (marker, state, state_color) = view_model::state_style(session.state);
            let info = div()
                .h(px(40.0))
                .px(px(16.0))
                .flex()
                .items_center()
                .gap(px(12.0))
                .border_b_1()
                .border_color(rgb(theme::HAIRLINE_STRONG))
                .child(Self::text(session.name.clone(), 13.0, theme::INK))
                .child(Self::text(format!("{marker} {state}"), 12.0, state_color))
                .child(Self::text(
                    view_model::agent_line(session),
                    12.0,
                    view_model::agent_color(session.key.cli),
                ))
                .child(Self::text(session.cwd.clone(), 12.0, theme::MUTE))
                .child(div().flex_1())
                .child(Self::text(
                    view_model::attach_line(session).unwrap_or_default(),
                    12.0,
                    theme::STONE,
                ));
            let view = self.terminals.get(&session.key).cloned();
            div().flex_1().flex().flex_col().child(info).child(div().flex_1().flex().bg(rgb(theme::TERMINAL_BG)).child(if let Some(view) = view { div().flex_1().child(view) } else { div().flex_1().flex().items_center().justify_center().child(Self::text(view_model::not_attachable_text(session.key.cli).0, 20.0, theme::INK)) }).child(div().w(px(260.0)).p(px(16.0)).border_l_1().border_color(rgb(theme::HAIRLINE_STRONG)).child(Self::text("DETALLE\n\nproyecto\nrol\nlanzada por\nestado desde\n\nHABLA CON\nsin comunicaciones conocidas", 12.0, theme::MUTE))))
        } else {
            div().flex_1().flex().items_center().justify_center().bg(rgb(theme::TERMINAL_BG)).child(Self::text("Nueva sesión\n\nen [#] proyecto con Claude Code\n\nEscribe el encargo. titan-agent no lo completa ni lo reescribe.\n\ncl Claude Code   opus   high   default   [#] proyecto   [ok] lanzar\n\ndisponible en la fase 2", 20.0, theme::INK))
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::CANVAS))
            .text_color(rgb(theme::BODY))
            .child(header)
            .child(div().flex_1().flex().child(self.sidebar()).child(center))
    }
}
