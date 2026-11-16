//! Main window (docs/Decisiones visuales/Ventana principal al estilo Claude
//! Code Desktop.md): header with counters, session tree on the left and the
//! selection in the centre.
//!
//! This module holds the start-up, the state shared by every view and the
//! routing (daemon messages, selection, keys, which view goes in the centre).
//! Each view lives in its own submodule: `header`, `nav` (tree and filter),
//! `session_view` (terminal), `detail` (session detail), `monitor` (project
//! monitor and its three views), `thread_view` (a team role's thread,
//! read-only), `new_session`, `settings`; `menu` and
//! `widgets` are pieces they share.

use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, KeyDownEvent, MouseButton,
    MouseDownEvent, Subscription, Window, div, prelude::*, px,
};
use titan_agent_core::launch::LaunchDefaults;
use titan_agent_core::protocol::{
    AttachResult, Event, LaunchResult, ServerMessage, SourceStatus, TeamTask, TerminalRef,
    TranscriptResult, decode_bytes,
};
use titan_agent_core::quota::Quota;
use titan_agent_core::session::{AgentCli, Session};

use crate::brief::{BriefEvent, BriefInput};
use crate::client::{DaemonClient, Incoming};
use crate::terminal::TerminalView;
use crate::theme::{self, color};

mod detail;
mod header;
mod menu;
mod monitor;
mod nav;
mod new_session;
mod session_view;
mod settings;
mod thread_view;
mod widgets;

use menu::Menu;
use monitor::MonitorTab;
use new_session::NewSession;
use thread_view::ThreadState;
use widgets::placeholder;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Selection {
    None,
    Session(String),
    Project(String),
    New,
    Settings,
    /// A team role's Codex thread, read-only (`cx:<thread>`).
    Thread(String),
}

enum Pending {
    Attach(String),
    Stop,
    Open,
    Launch,
    Transcript(String),
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
    focus: FocusHandle,
    new: NewSession,
    /// Defaults of the "new session" box, edited from `[*]`.
    defaults: LaunchDefaults,
    settings_menu: Option<(AgentCli, Menu)>,
    /// Folders picked with `[d]`, offered next to the projects.
    folders: Vec<String>,
    /// Focus the brief on the next frame.
    focus_brief: bool,
    /// Real session of each launch, from the last snapshot (it can arrive
    /// before the launch's own answer).
    resolved: HashMap<String, String>,
    /// Open team tasks and CLI quotas, from the last snapshot.
    teams: Vec<TeamTask>,
    quotas: Vec<Quota>,
    /// Terminals alive in the daemon, from the last snapshot.
    daemon_terminals: Vec<TerminalRef>,
    /// The thread shown read-only (or last shown).
    thread: Option<ThreadState>,
    monitor_tab: MonitorTab,
    /// Time window of the monitor's lanes, in seconds.
    lane_window_s: i64,
    /// Window width, for views that place things in pixels.
    viewport_w: f32,
    _brief_events: Subscription,
}

fn now_ms() -> i64 {
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
                        view.refresh_thread();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        // The monitor moves (dots, lanes, spinner): redraw it often, and
        // only while it is on screen.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this
                    .update(cx, |view, cx| {
                        if view.monitor_animating() {
                            view.now_ms = now_ms();
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let preselect = std::env::var("TITAN_AGENT_SELECT").ok();
        let defaults = crate::config::load();
        let brief = cx.new(|cx| {
            let mut input = BriefInput::new(
                "Escribe el encargo. titan-agent no lo completa ni lo reescribe.",
                cx,
            );
            // Screenshots only: text typed beforehand by a person, shown in
            // the field. It is never launched without a click.
            if let Ok(text) = std::env::var("TITAN_AGENT_DEMO_BRIEF") {
                input.content.text = text.replace("\\n", "\n");
                input.content.move_to(input.content.text.len());
            }
            input
        });
        let brief_events = cx.subscribe(&brief, |this, _, event: &BriefEvent, cx| match event {
            BriefEvent::Changed => {
                this.new.error = None;
                cx.notify();
            }
            BriefEvent::Submit => this.launch(cx),
        });
        let demo_menu = match std::env::var("TITAN_AGENT_DEMO_MENU").as_deref() {
            Ok("agent") => Some(Menu::Agent),
            Ok("model") => Some(Menu::Model),
            Ok("effort") => Some(Menu::Effort),
            Ok("permission") => Some(Menu::Permission),
            Ok("project") => Some(Menu::Project),
            _ => None,
        };
        MainView {
            client,
            connected: false,
            status: Some("conectando con titan-agentd…".into()),
            sessions: Vec::new(),
            sources: Vec::new(),
            selection: if std::env::var("TITAN_AGENT_NEW").is_ok() {
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
            home: std::env::var("HOME").ok(),
            preselect,
            focus_terminal: false,
            focus: cx.focus_handle(),
            new: NewSession {
                brief,
                form: defaults.clone(),
                folder: std::env::var("TITAN_AGENT_FOLDER").ok(),
                menu: demo_menu,
                launching: false,
                sent: None,
                // Screenshots only: how a refused launch looks.
                error: std::env::var("TITAN_AGENT_DEMO_ERROR").ok(),
            },
            defaults,
            settings_menu: None,
            folders: Vec::new(),
            focus_brief: true,
            resolved: HashMap::new(),
            teams: Vec::new(),
            quotas: Vec::new(),
            daemon_terminals: Vec::new(),
            thread: None,
            monitor_tab: MonitorTab::from_env(),
            lane_window_s: monitor::LANE_WINDOWS[0],
            viewport_w: 1440.,
            _brief_events: brief_events,
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
                self.status = Some(reason);
                // The answer will never come: say so instead of waiting, and
                // never send it again on our own.
                let before = self.pending.len();
                self.pending.retain(|_, p| !matches!(p, Pending::Launch));
                if self.new.launching || self.pending.len() != before {
                    self.new.launching = false;
                    self.new.sent = None;
                    self.new.error = Some(
                        "se perdió la conexión con titan-agentd: no se sabe si la sesión llegó a lanzarse; mira el árbol antes de repetir"
                            .into(),
                    );
                }
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
                Some(Pending::Launch) => {
                    self.new.launching = false;
                    match serde_json::from_value::<LaunchResult>(result) {
                        Ok(launched) => self.launched(launched, cx),
                        Err(error) => self.new.error = Some(error.to_string()),
                    }
                }
                Some(Pending::Transcript(key)) => self.transcript_answer(
                    &key,
                    serde_json::from_value::<TranscriptResult>(result).map_err(|error| {
                        titan_agent_core::protocol::ErrorBody {
                            code: "bad_answer".into(),
                            message: error.to_string(),
                        }
                    }),
                ),
                Some(Pending::Open) | None => {}
            },
            ServerMessage::Error { id, error } => {
                match id.and_then(|id| self.pending.remove(&id)) {
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
                    Some(Pending::Launch) => {
                        self.new.launching = false;
                        self.new.sent = None;
                        self.new.error = Some(error.message);
                    }
                    Some(Pending::Transcript(key)) => self.transcript_answer(&key, Err(error)),
                    _ => self.notice = Some((error.message, true)),
                }
            }
            ServerMessage::Event(Event::Sessions {
                sessions,
                sources,
                launches,
                teams,
                quotas,
                terminals,
            }) => {
                self.daemon_terminals = terminals;
                self.sessions = sessions;
                self.sources = sources;
                self.teams = teams;
                self.quotas = quotas;
                self.follow_launches(&launches, cx);
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
                self.preselect_thread(cx);
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

    fn root_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.control && keystroke.key == "n" {
            self.open_new(window, cx);
        } else if keystroke.key == "escape" && self.new.menu.is_some() {
            self.new.menu = None;
            cx.notify();
        } else if keystroke.key == "escape" && self.cancel_take_control() {
            cx.notify();
        }
    }

    fn session(&self, key: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.key == key)
    }

    fn center(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.selection.clone() {
            Selection::Thread(_) => self.thread_view(cx),
            Selection::Session(key) => match self
                .session(&key)
                .cloned()
                .or_else(|| self.thread_session(&key))
            {
                Some(session) => self.session_view(&session, window, cx).into_any_element(),
                None => placeholder(
                    "La sesión ya no aparece",
                    vec!["Puede que haya terminado. Elige otra en el árbol.".into()],
                )
                .into_any_element(),
            },
            Selection::Project(root) => self.monitor_view(&root, cx).into_any_element(),
            Selection::New => self.new_session_view(window, cx).into_any_element(),
            Selection::Settings => self.settings_view(cx).into_any_element(),
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
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.viewport_w = f32::from(window.viewport_size().width);
        if self.focus_terminal {
            self.focus_terminal = false;
            if let Selection::Session(key) = &self.selection
                && let Some(view) = self.terminals.get(key)
            {
                let handle = view.read(cx).focus_handle(cx);
                handle.focus(window, cx);
            }
        }
        if self.focus_brief && self.selection == Selection::New {
            self.focus_brief = false;
            let handle = self.new.brief.read(cx).focus_handle(cx);
            handle.focus(window, cx);
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
