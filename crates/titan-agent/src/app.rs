//! Main window (docs/Decisiones visuales/Ventana principal al estilo Claude
//! Code Desktop.md): header with counters, session tree on the left and the
//! selection in the centre.

use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, ElementId, Entity, FocusHandle, Focusable,
    FontWeight, KeyDownEvent, MouseButton, MouseDownEvent, SharedString, Stateful, Window, div,
    prelude::*, px,
};
use titan_agent_core::launch::{LaunchDefaults, LaunchSpec};
use titan_agent_core::project::tilde;
use titan_agent_core::protocol::{
    AttachResult, Event, LaunchResult, Request, ServerMessage, SourceStatus, TerminalRef,
    decode_bytes,
};
use titan_agent_core::session::{AgentCli, Session, SessionKind, SessionState, StateCounts};
use titan_agent_core::time::ago;
use titan_agent_core::tree::{TreeRow, build_rows};

use crate::client::{DaemonClient, Incoming};
use crate::new_session::{NewSessionEvent, NewSessionView, ProjectChoice};
use crate::settings::{SettingsEvent, SettingsView};
use crate::terminal::TerminalView;
use crate::theme::{self, color, hairline};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Selection {
    None,
    Session(String),
    Project(String),
    New,
    Settings,
    /// A Claude Code session just launched, until it shows up in the tree.
    Launched {
        cli_id: Option<String>,
        command: String,
    },
}

enum Pending {
    Attach(String),
    Stop,
    Open,
    Launch(LaunchSpec),
}

pub struct MainView {
    client: DaemonClient,
    connected: bool,
    status: Option<String>,
    sessions: Vec<Session>,
    sources: Vec<SourceStatus>,
    selection: Selection,
    filter: String,
    filter_focus: FocusHandle,
    terminals: HashMap<String, Entity<TerminalView>>,
    pending: HashMap<u64, Pending>,
    notice: Option<(String, bool)>,
    now_ms: i64,
    home: Option<String>,
    /// Name fragment of a session to select once it appears
    /// (`TITAN_AGENT_SELECT`), used to open the window on a given session.
    preselect: Option<String>,
    /// Focus the selected terminal on the next frame (selection made
    /// without a window at hand).
    focus_terminal: bool,
    /// Focus the brief of «Nueva sesión» on the next frame.
    focus_brief: bool,
    focus: FocusHandle,
    new_session: Entity<NewSessionView>,
    settings: Entity<SettingsView>,
    /// Codex TUIs launched from here, shown until their thread is found.
    launched_tuis: HashMap<String, Session>,
    _subscriptions: Vec<gpui::Subscription>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

impl MainView {
    pub fn new(
        client: DaemonClient,
        incoming: UnboundedReceiver<Incoming>,
        cx: &mut Context<Self>,
    ) -> MainView {
        cx.spawn(async move |this, cx| {
            let mut incoming = incoming;
            while let Some(message) = incoming.next().await {
                if this
                    .update(cx, |view, cx| view.receive(message, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this
                    .update(cx, |view, cx| {
                        view.now_ms = now_ms();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let preselect = std::env::var("TITAN_AGENT_SELECT").ok();
        let home = std::env::var("HOME").ok();
        let defaults = crate::settings::load_defaults();
        let new_session = cx.new(|cx| NewSessionView::new(&defaults, home.clone(), cx));
        let settings = cx.new(|_| SettingsView::new(defaults.clone(), home.clone()));
        let subscriptions = vec![
            cx.subscribe(&new_session, |this, _, event, cx| match event {
                NewSessionEvent::Launch(request) => this.launch(request, cx),
            }),
            cx.subscribe(&settings, |this, _, event, cx| match event {
                SettingsEvent::Changed(defaults) => this.defaults_changed(defaults.clone(), cx),
            }),
        ];
        let start_new = std::env::var("TITAN_AGENT_NEW").is_ok();
        MainView {
            client,
            connected: false,
            status: Some("conectando con titan-agentd…".into()),
            sessions: Vec::new(),
            sources: Vec::new(),
            selection: if start_new {
                Selection::New
            } else if std::env::var("TITAN_AGENT_SETTINGS").is_ok() {
                Selection::Settings
            } else {
                Selection::None
            },
            filter: std::env::var("TITAN_AGENT_FILTER").unwrap_or_default(),
            filter_focus: cx.focus_handle(),
            terminals: HashMap::new(),
            pending: HashMap::new(),
            notice: None,
            now_ms: now_ms(),
            home,
            preselect,
            focus_terminal: false,
            focus_brief: start_new,
            focus: cx.focus_handle(),
            new_session,
            settings,
            launched_tuis: HashMap::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Projects offered in «Nueva sesión», in tree order.
    fn projects(&self) -> Vec<ProjectChoice> {
        build_rows(&self.sessions, "")
            .into_iter()
            .filter_map(|row| match row {
                TreeRow::Project { name, root, .. } => Some(ProjectChoice { root, name }),
                TreeRow::Session { .. } => None,
            })
            .collect()
    }

    /// Opens «Nueva sesión» in the project of the current selection.
    fn open_new(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        let preferred = match &self.selection {
            Selection::Session(key) => self.session(key).map(|s| s.project_root.clone()),
            Selection::Project(root) => Some(root.clone()),
            _ => None,
        };
        let projects = self.projects();
        self.new_session.update(cx, |form, cx| {
            form.set_projects(projects, preferred, cx);
        });
        self.selection = Selection::New;
        self.notice = None;
        match window {
            Some(window) => {
                let handle = self.new_session.read(cx).focus_handle(cx);
                handle.focus(window, cx);
            }
            None => self.focus_brief = true,
        }
        cx.notify();
    }

    fn defaults_changed(&mut self, defaults: LaunchDefaults, cx: &mut Context<Self>) {
        self.new_session.update(cx, |form, cx| {
            form.apply_defaults(&defaults);
            cx.notify();
        });
    }

    /// Sends one launch to the daemon for one click (ADR-0002).
    fn launch(&mut self, request: &crate::new_session::LaunchRequest, cx: &mut Context<Self>) {
        if !self.connected {
            self.new_session.update(cx, |form, cx| {
                form.failed("sin conexión con titan-agentd".into(), cx)
            });
            return;
        }
        let id = self.client.request(Request::Launch {
            spec: request.spec.clone(),
            shown: request.shown.clone(),
            click: request.click.clone(),
            cols: 120,
            rows: 36,
        });
        self.pending
            .insert(id, Pending::Launch(request.spec.clone()));
    }

    fn launched(&mut self, result: LaunchResult, spec: LaunchSpec, cx: &mut Context<Self>) {
        self.new_session.update(cx, |form, cx| form.launched(cx));
        match result.terminal {
            Some(terminal) => {
                // Codex: its TUI already runs in a daemon terminal, listed
                // under a provisional key until the thread shows up.
                let key = titan_agent_core::protocol::provisional_key(&terminal);
                let client = self.client.clone();
                let view = cx.new(|cx| TerminalView::new(key.clone(), client, cx));
                view.update(cx, |view, cx| view.attached(terminal, true, cx));
                self.terminals.insert(key.clone(), view);
                self.launched_tuis
                    .insert(key.clone(), provisional_session(&key, &spec));
                self.selection = Selection::Session(key);
                self.focus_terminal = true;
            }
            None => {
                let existing = result.cli_id.as_ref().and_then(|id| {
                    self.sessions
                        .iter()
                        .find(|s| s.agent == result.agent && &s.cli_id == id)
                        .map(|s| s.key.clone())
                });
                match existing {
                    Some(key) => self.select_session(key, None, cx),
                    None => {
                        self.selection = Selection::Launched {
                            cli_id: result.cli_id,
                            command: result.command,
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    /// Follows the terminals the daemon moved to another session (a launched
    /// Codex TUI whose thread was found) and selects a launched Claude Code
    /// session once it shows up.
    fn follow_launches(&mut self, terminals: &[TerminalRef], cx: &mut Context<Self>) {
        for reference in terminals {
            let moved = self
                .terminals
                .iter()
                .find(|(key, view)| {
                    view.read(cx).terminal.as_deref() == Some(reference.terminal.as_str())
                        && **key != reference.session
                })
                .map(|(key, _)| key.clone());
            if let Some(old) = moved
                && let Some(view) = self.terminals.remove(&old)
            {
                let new = reference.session.clone();
                view.update(cx, |view, _| view.session = new.clone());
                self.terminals.insert(new.clone(), view);
                self.launched_tuis.remove(&old);
                if self.selection == Selection::Session(old) {
                    self.selection = Selection::Session(new);
                }
            }
        }
        if let Selection::Launched {
            cli_id: Some(id), ..
        } = &self.selection
            && let Some(key) = self
                .sessions
                .iter()
                .find(|s| s.agent == AgentCli::ClaudeCode && &s.cli_id == id)
                .map(|s| s.key.clone())
        {
            self.select_session(key, None, cx);
        }
    }

    fn receive(&mut self, incoming: Incoming, cx: &mut Context<Self>) {
        match incoming {
            Incoming::Connected => {
                self.connected = true;
                self.status = None;
                // A new connection has no terminals attached: attach again
                // the ones that were open.
                let open: Vec<String> = self
                    .terminals
                    .iter()
                    .filter(|(_, view)| view.read(cx).ended.is_none())
                    .map(|(key, _)| key.clone())
                    .collect();
                for key in open {
                    self.attach(&key, cx);
                }
            }
            Incoming::Disconnected(reason) => {
                self.connected = false;
                // A launch without an answer is not retried: tell the user.
                let before = self.pending.len();
                self.pending
                    .retain(|_, pending| !matches!(pending, Pending::Launch(_)));
                if self.pending.len() != before {
                    self.new_session.update(cx, |form, cx| {
                        form.failed(
                            "se perdió la conexión con titan-agentd antes de saber si se lanzó; \
                             revisa el árbol antes de volver a lanzar"
                                .into(),
                            cx,
                        )
                    });
                }
                self.status = Some(reason);
            }
            Incoming::Message(message) => self.message(message, cx),
        }
        cx.notify();
    }

    fn message(&mut self, message: ServerMessage, cx: &mut Context<Self>) {
        match message {
            ServerMessage::Result { id, result } => match self.pending.remove(&id) {
                Some(Pending::Attach(session)) => {
                    if let (Ok(answer), Some(view)) = (
                        serde_json::from_value::<AttachResult>(result),
                        self.terminals.get(&session),
                    ) {
                        view.update(cx, |view, cx| {
                            view.attached(answer.terminal, answer.created, cx)
                        });
                    }
                }
                Some(Pending::Stop) => {
                    self.notice = Some(("orden de detener enviada al CLI".into(), false));
                }
                Some(Pending::Launch(spec)) => match serde_json::from_value::<LaunchResult>(result)
                {
                    Ok(result) => self.launched(result, spec, cx),
                    Err(error) => self.new_session.update(cx, |form, cx| {
                        form.failed(format!("respuesta inesperada del daemon: {error}"), cx)
                    }),
                },
                Some(Pending::Open) | None => {}
            },
            ServerMessage::Error { id, error } => match self.pending.remove(&id) {
                Some(Pending::Launch(_)) => {
                    self.new_session
                        .update(cx, |form, cx| form.failed(error.message, cx));
                }
                Some(Pending::Attach(session)) => {
                    if let Some(view) = self.terminals.get(&session) {
                        let text = error.message.clone();
                        view.update(cx, |view, cx| {
                            view.ended = Some(text);
                            cx.notify();
                        });
                    }
                    self.notice = Some((error.message, true));
                }
                _ => self.notice = Some((error.message, true)),
            },
            ServerMessage::Event(Event::Sessions {
                sessions,
                sources,
                terminals,
            }) => {
                self.sessions = sessions;
                self.sources = sources;
                self.follow_launches(&terminals, cx);
                let projects = self.projects();
                self.new_session
                    .update(cx, |form, _| form.refresh_projects(projects));
                if let Some(fragment) = self.preselect.clone()
                    && let Some(key) = self
                        .sessions
                        .iter()
                        .find(|s| s.name.contains(&fragment))
                        .map(|s| s.key.clone())
                {
                    self.preselect = None;
                    self.select_session(key, None, cx);
                }
                if let Ok(name) = std::env::var("TITAN_AGENT_PROJECT")
                    && self.selection == Selection::None
                    && let Some(root) = self
                        .sessions
                        .iter()
                        .find(|s| s.project == name)
                        .map(|s| s.project_root.clone())
                {
                    self.selection = Selection::Project(root);
                }
            }
            ServerMessage::Event(Event::Output { terminal, data }) => {
                if let (Some(view), Some(bytes)) =
                    (self.view_for_terminal(&terminal, cx), decode_bytes(&data))
                {
                    view.update(cx, |view, cx| view.feed(&bytes, cx));
                }
            }
            ServerMessage::Event(Event::TerminalClosed {
                terminal,
                exit_code,
                ..
            }) => {
                if let Some(view) = self.view_for_terminal(&terminal, cx) {
                    view.update(cx, |view, cx| view.closed(exit_code, cx));
                }
            }
        }
    }

    fn view_for_terminal(&self, terminal: &str, cx: &App) -> Option<Entity<TerminalView>> {
        self.terminals
            .values()
            .find(|view| view.read(cx).terminal.as_deref() == Some(terminal))
            .cloned()
    }

    fn attach(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(view) = self.terminals.get(key) {
            let id = view.update(cx, |view, _| view.attach());
            self.pending.insert(id, Pending::Attach(key.to_string()));
        }
    }

    fn select_session(&mut self, key: String, window: Option<&mut Window>, cx: &mut Context<Self>) {
        self.selection = Selection::Session(key.clone());
        self.notice = None;
        let attachable = self
            .sessions
            .iter()
            .any(|s| s.key == key && s.attach.is_some());
        if attachable && !self.terminals.contains_key(&key) {
            let client = self.client.clone();
            let session = key.clone();
            let view = cx.new(|cx| TerminalView::new(session, client, cx));
            self.terminals.insert(key.clone(), view);
            self.attach(&key, cx);
        }
        match (window, self.terminals.get(&key)) {
            (Some(window), Some(view)) => {
                let handle = view.read(cx).focus_handle(cx);
                handle.focus(window, cx);
            }
            (None, Some(_)) => self.focus_terminal = true,
            _ => {}
        }
        cx.notify();
    }

    fn reopen(&mut self, key: &str, cx: &mut Context<Self>) {
        self.attach(key, cx);
        cx.notify();
    }

    fn filter_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
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

    fn root_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.control && keystroke.key == "n" {
            self.open_new(Some(window), cx);
            cx.stop_propagation();
        } else if keystroke.key == "escape" {
            let closed = self.new_session.update(cx, |form, cx| form.close_menu(cx))
                || self.settings.update(cx, |view, cx| view.close_menu(cx));
            if closed {
                cx.stop_propagation();
            }
        }
    }

    fn session(&self, key: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.key == key)
    }

    // ----- rendering -------------------------------------------------------

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
                    .when(self.selection == Selection::Settings, |d| {
                        d.text_color(color(theme::ACCENT))
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.selection = Selection::Settings;
                        this.notice = None;
                        cx.notify();
                    })),
            )
    }

    fn nav(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_new(Some(window), cx);
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

    fn center(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.selection.clone() {
            Selection::Session(key) => match self
                .session(&key)
                .or_else(|| self.launched_tuis.get(&key))
                .cloned()
            {
                Some(session) => self.session_view(&session, window, cx).into_any_element(),
                None => placeholder(
                    "La sesión ya no aparece",
                    vec!["Puede que haya terminado. Elige otra en el árbol.".into()],
                )
                .into_any_element(),
            },
            Selection::Project(root) => self.project_view(&root).into_any_element(),
            Selection::New => self.new_session.clone().into_any_element(),
            Selection::Settings => self.settings.clone().into_any_element(),
            Selection::Launched { cli_id, command } => placeholder(
                "Sesión lanzada",
                vec![
                    match cli_id {
                        Some(id) => format!(
                            "claude --bg respondió con el id {id}; aparecerá en el árbol en unos segundos."
                        ),
                        None => "claude --bg no devolvió un id: búscala en el árbol.".into(),
                    },
                    format!("$ {command}"),
                ],
            )
            .into_any_element(),
            Selection::None => placeholder(
                "Elige una sesión",
                vec![
                    "Su terminal aparece aquí y lo que escribes va directo al CLI oficial.".into(),
                    "Pulsa un proyecto para ver su monitor.".into(),
                ],
            )
            .into_any_element(),
        }
    }

    fn session_view(
        &mut self,
        session: &Session,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let agent = theme::agent_color(session.agent);
        let model = session.model.clone().unwrap_or_else(|| "modelo ?".into());
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
            );

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

    fn detail(&self, session: &Session, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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

    fn project_view(&self, root: &str) -> impl IntoElement + use<> {
        let sessions: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|s| s.project_root == root)
            .collect();
        let name = sessions
            .first()
            .map(|s| s.project.clone())
            .unwrap_or_else(|| titan_agent_core::project::project_name(root));
        let team = sessions
            .iter()
            .any(|s| s.parent.is_some() && s.state.is_live());
        let subtitle = if team {
            let members = sessions.iter().filter(|s| s.state.is_live()).count();
            format!("{members} agentes en vivo")
        } else {
            format!(
                "{} sesión(es), ninguna en equipo ahora",
                sessions.iter().filter(|s| s.parent.is_none()).count()
            )
        };
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
            .child(
                div()
                    .text_color(color(theme::INK))
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(13.))
                    .child(format!("[#] {name}")),
            )
            .child(subtitle)
            .when(team, |d| {
                d.child(marked("[*]", theme::SUCCESS, "en vivo").text_color(color(theme::SUCCESS)))
            });
        let title = if team {
            format!("Equipo trabajando en {name}")
        } else {
            format!("Sin equipo trabajando en {name}")
        };
        let mut lines = vec![if team {
            "Las vistas grafo, carriles y mesa de control llegan en la fase 3.".to_string()
        } else {
            "El monitor se anima cuando las sesiones del proyecto se encargan trabajo entre ellas."
                .to_string()
        }];
        // Top-level sessions by name; threads launched by another one are
        // only counted, so a long team run does not flood the panel.
        let top: Vec<String> = sessions
            .iter()
            .filter(|s| s.parent.is_none())
            .map(|s| s.name.clone())
            .collect();
        let nested = sessions.len() - top.len();
        let shown = top.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
        let more = top.len().saturating_sub(6);
        let mut list = format!(
            "{}: {shown}",
            if team { "Sesiones" } else { "Sesiones sueltas" }
        );
        if more > 0 {
            list.push_str(&format!(" y {more} más"));
        }
        lines.push(list);
        if nested > 0 {
            lines.push(format!(
                "{nested} hilo(s) lanzados por otras sesiones en los últimos 20 min."
            ));
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(bar)
            .child(placeholder(&title, lines))
    }
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_brief {
            self.focus_brief = false;
            let handle = self.new_session.read(cx).focus_handle(cx);
            handle.focus(window, cx);
        }
        if self.focus_terminal {
            self.focus_terminal = false;
            if let Selection::Session(key) = &self.selection
                && let Some(view) = self.terminals.get(key)
            {
                let handle = view.read(cx).focus_handle(cx);
                handle.focus(window, cx);
            }
        }
        let nav = self.nav(window, cx);
        let center = self.center(window, cx);
        div()
            .id("main")
            .key_context("Main")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::root_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, _| {}),
            )
            .size_full()
            .flex()
            .flex_col()
            .bg(color(theme::CANVAS))
            .text_color(color(theme::BODY))
            .font_family(theme::FONT)
            .font_features(gpui::FontFeatures::disable_ligatures())
            .text_size(px(14.))
            .child(self.header(cx))
            .child(
                div().flex().flex_grow_1().min_h(px(0.)).child(nav).child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_grow_1()
                        .min_w(px(0.))
                        .child(center),
                ),
            )
    }
}

// ----- small pieces ---------------------------------------------------------

/// What is shown of a Codex TUI launched from here until its thread appears.
fn provisional_session(key: &str, spec: &LaunchSpec) -> Session {
    use titan_agent_core::project::{project_name, project_root};
    let root = project_root(&spec.cwd);
    let now = now_ms();
    Session {
        key: key.to_string(),
        agent: spec.agent,
        cli_id: String::new(),
        name: titan_agent_core::launch::session_name(&spec.brief),
        cwd: spec.cwd.clone(),
        project: project_name(&root),
        project_root: root,
        kind: SessionKind::Interactive,
        state: SessionState::Working,
        model: Some(titan_agent_core::session::short_model(&spec.model)),
        effort: Some(spec.effort.clone()),
        role: None,
        parent: None,
        started_at_ms: Some(now),
        state_since_ms: Some(now),
        pid: None,
        attach: None,
        stoppable: false,
        launched_here: true,
    }
}

/// ASCII marker of a state. The spinner `| / - \` belongs to the monitor
/// (phase 3); the tree and the bar show the plain marker.
fn state_marker(state: SessionState, _now_ms: i64) -> String {
    state.marker().to_string()
}

fn marked(marker: &str, marker_color: u32, text: impl Into<SharedString>) -> Div {
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

fn caret() -> Div {
    div().w(px(1.)).h(px(14.)).bg(color(theme::ACCENT))
}

fn agent_pill(agent: AgentCli, model: Option<&str>, agent_color: u32) -> Div {
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

fn button(id: &'static str, label: &'static str, border: u32, text: u32) -> Stateful<Div> {
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

fn placeholder(title: &str, lines: Vec<String>) -> Div {
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

fn not_attachable(session: &Session) -> Div {
    let mut lines = Vec::new();
    match (session.agent, session.kind) {
        (AgentCli::ClaudeCode, SessionKind::Interactive) => {
            lines.push(match session.pid {
                Some(pid) => {
                    format!("Es una sesión interactiva: vive en su propio terminal (pid {pid}).")
                }
                None => "Es una sesión interactiva: vive en su propio terminal.".into(),
            });
            lines
                .push("claude attach solo adjunta sesiones en segundo plano (claude --bg).".into());
        }
        _ => lines.push("El CLI no ofrece un comando para adjuntar esta sesión.".into()),
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
                .child("Sin terminal adjuntable"),
        )
        .children(lines.into_iter().map(|line| {
            div()
                .text_color(color(theme::MUTE))
                .text_size(px(13.))
                .child(line)
        }))
}
