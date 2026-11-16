//! Main window (docs/Decisiones visuales/Ventana principal al estilo Claude
//! Code Desktop.md): header with counters, session tree on the left and the
//! selection in the centre.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, ElementId, Entity, FocusHandle, Focusable,
    FontWeight, KeyDownEvent, MouseButton, MouseDownEvent, PathPromptOptions, Role, SharedString,
    Stateful, Subscription, Window, div, prelude::*, px,
};
use titan_agent_core::launch::{
    LaunchDefaults, LaunchSpec, efforts, folder_name, models, permissions,
};
use titan_agent_core::project::{project_name, tilde};
use titan_agent_core::protocol::{
    AttachResult, Event, LaunchRef, LaunchResult, Request, ServerMessage, SourceStatus,
    decode_bytes,
};
use titan_agent_core::session::{AgentCli, Session, SessionKind, SessionState, StateCounts};
use titan_agent_core::time::ago;
use titan_agent_core::tree::{TreeRow, build_rows};

use crate::brief::{BriefEvent, BriefInput};
use crate::client::{DaemonClient, Incoming};
use crate::terminal::TerminalView;
use crate::theme::{self, color, hairline};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Selection {
    None,
    Session(String),
    Project(String),
    New,
    Settings,
}

enum Pending {
    Attach(String),
    Stop,
    Open,
    Launch,
}

/// The setting a button of the "new session" box opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Menu {
    Agent,
    Model,
    Effort,
    Permission,
    Project,
}

/// State of the "new session" box (ADR-0002: the brief is only what the
/// user types; one click, one launch; the exact command is shown).
struct NewSession {
    brief: Entity<BriefInput>,
    /// Current choices, starting from the configured defaults.
    form: LaunchDefaults,
    /// Working folder (a project root or a folder picked with `[d]`).
    folder: Option<String>,
    menu: Option<Menu>,
    /// A launch request is on its way: the button does nothing meanwhile.
    launching: bool,
    /// The brief of the launch on its way, to empty the field only if it
    /// was not edited meanwhile.
    sent: Option<String>,
    error: Option<String>,
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
                    _ => self.notice = Some((error.message, true)),
                }
            }
            ServerMessage::Event(Event::Sessions {
                sessions,
                sources,
                launches,
                ..
            }) => {
                self.sessions = sessions;
                self.sources = sources;
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
            self.open_new(window, cx);
        } else if keystroke.key == "escape" && self.new.menu.is_some() {
            self.new.menu = None;
            cx.notify();
        }
    }

    // ----- new session -----------------------------------------------------

    /// Shows the "new session" box, in the project of the current selection
    /// when there is one.
    fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = match &self.selection {
            Selection::Session(key) => self.session(key).map(|s| s.project_root.clone()),
            Selection::Project(root) => Some(root.clone()),
            _ => None,
        };
        if let Some(root) = current {
            self.new.folder = Some(root);
        }
        self.selection = Selection::New;
        self.new.menu = None;
        let handle = self.new.brief.read(cx).focus_handle(cx);
        handle.focus(window, cx);
        cx.notify();
    }

    /// Projects offered as working folder: every project in the tree, then
    /// the folders picked with `[d]`. As `(folder, name)`.
    fn folder_choices(&self) -> Vec<(String, String)> {
        let mut choices: Vec<(String, String)> = Vec::new();
        for session in &self.sessions {
            if !choices
                .iter()
                .any(|(root, _)| *root == session.project_root)
            {
                choices.push((session.project_root.clone(), session.project.clone()));
            }
        }
        choices.sort_by(|a, b| a.1.cmp(&b.1));
        for folder in &self.folders {
            if !choices.iter().any(|(root, _)| root == folder) {
                choices.push((folder.clone(), folder_name(folder).to_string()));
            }
        }
        choices
    }

    fn folder(&self) -> Option<String> {
        self.new
            .folder
            .clone()
            .or_else(|| self.folder_choices().first().map(|(root, _)| root.clone()))
    }

    /// The spec the box would launch now. With an empty brief, the command
    /// line shows a marker instead (it cannot be launched).
    fn spec(&self, cx: &App) -> Option<LaunchSpec> {
        let agent = self.new.form.agent;
        let chosen = self.new.form.get(agent).clone();
        Some(LaunchSpec {
            agent,
            cwd: self.folder()?,
            model: chosen.model,
            effort: chosen.effort,
            permission: chosen.permission,
            brief: self.new.brief.read(cx).text().to_string(),
        })
    }

    /// One click of `[ok] lanzar` (or ctrl+enter): one launch request with
    /// a fresh token. Nothing is retried.
    fn launch(&mut self, cx: &mut Context<Self>) {
        if self.new.launching {
            return;
        }
        if !self.connected {
            self.new.error = Some("sin conexión con titan-agentd".into());
            cx.notify();
            return;
        }
        let Some(spec) = self.spec(cx) else {
            self.new.error = Some("elige la carpeta de trabajo con [#] o [d]".into());
            cx.notify();
            return;
        };
        if let Err(error) = spec.validate() {
            self.new.error = Some(error.to_string());
            cx.notify();
            return;
        }
        self.new.sent = Some(spec.brief.clone());
        let id = self.client.request(Request::Launch {
            token: new_token(),
            command: spec.command_line(),
            spec,
        });
        self.pending.insert(id, Pending::Launch);
        self.new.launching = true;
        self.new.error = None;
        self.new.menu = None;
        cx.notify();
    }

    /// The daemon launched it: select its provisional session (with the
    /// TUI's terminal for Codex) and empty the box for the next one.
    fn launched(&mut self, launched: LaunchResult, cx: &mut Context<Self>) {
        let sent = self.new.sent.take();
        if sent.as_deref() == Some(self.new.brief.read(cx).text()) {
            self.new.brief.update(cx, |brief, cx| brief.clear(cx));
        }
        // The snapshot may already have told us the real session.
        let key = self
            .resolved
            .get(&launched.launch)
            .cloned()
            .unwrap_or(launched.launch);
        if let Some(terminal) = launched.terminal {
            let client = self.client.clone();
            let session = key.clone();
            let view = cx.new(|cx| TerminalView::new(session, client, cx));
            view.update(cx, |view, cx| view.adopt(terminal, cx));
            self.terminals.insert(key.clone(), view);
        }
        if key.starts_with("launch:") {
            self.selection = Selection::Session(key);
            self.focus_terminal = true;
            cx.notify();
        } else {
            self.select_session(key, None, cx);
        }
    }

    /// Moves what belongs to a launch (selection, terminal) to the real
    /// session once the daemon knows it.
    fn follow_launches(&mut self, launches: &[LaunchRef], cx: &mut Context<Self>) {
        for launch in launches {
            let Some(real) = &launch.session else {
                continue;
            };
            self.resolved.insert(launch.launch.clone(), real.clone());
            if let Some(view) = self.terminals.remove(&launch.launch) {
                view.update(cx, |view, _| view.session = real.clone());
                self.terminals.entry(real.clone()).or_insert(view);
            }
            if self.selection == Selection::Session(launch.launch.clone()) {
                self.select_session(real.clone(), None, cx);
            }
        }
    }

    fn pick_folder(&mut self, cx: &mut Context<Self>) {
        self.new.menu = None;
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Elegir carpeta de trabajo".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = answer.await
                && let Some(path) = paths.into_iter().next()
            {
                let folder = path.to_string_lossy().into_owned();
                let _ = this.update(cx, |view, cx| {
                    if !view.folders.contains(&folder) {
                        view.folders.push(folder.clone());
                    }
                    view.new.folder = Some(folder);
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn set_default(&mut self, agent: AgentCli, menu: Menu, value: &str) {
        match menu {
            Menu::Agent => {
                if let Some(agent) = AgentCli::ALL.into_iter().find(|a| a.tag() == value) {
                    self.defaults.agent = agent;
                }
            }
            Menu::Model => self.defaults.get_mut(agent).model = value.into(),
            Menu::Effort => self.defaults.get_mut(agent).effort = value.into(),
            Menu::Permission => self.defaults.get_mut(agent).permission = value.into(),
            Menu::Project => {}
        }
        self.settings_menu = None;
        self.notice = Some(match crate::config::save(&self.defaults) {
            Ok(()) => ("valores por defecto guardados".into(), false),
            Err(error) => (format!("no se pudieron guardar: {error}"), true),
        });
        // The box starts from the new defaults unless it is being filled.
        self.new.form = self.defaults.clone();
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
                    .role(Role::Button)
                    .aria_label("configuración")
                    .text_color(color(if self.selection == Selection::Settings {
                        theme::ACCENT
                    } else {
                        theme::BODY
                    }))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.selection = Selection::Settings;
                        this.settings_menu = None;
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
            Selection::Session(key) => match self.session(&key).cloned() {
                Some(session) => self.session_view(&session, window, cx).into_any_element(),
                None => placeholder(
                    "La sesión ya no aparece",
                    vec!["Puede que haya terminado. Elige otra en el árbol.".into()],
                )
                .into_any_element(),
            },
            Selection::Project(root) => self.project_view(&root).into_any_element(),
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

    fn new_session_view(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let agent = self.new.form.agent;
        let chosen = self.new.form.get(agent).clone();
        let agent_color = theme::agent_color(agent);
        let folder = self.folder();
        let project = folder
            .as_deref()
            .map(project_name)
            .unwrap_or_else(|| "sin carpeta".into());
        let empty = self.new.brief.read(cx).text().trim().is_empty();
        let command = self.spec(cx).map(|mut spec| {
            if spec.brief.trim().is_empty() {
                spec.brief = "<tu encargo>".into();
            }
            spec.command_line()
        });
        let open = self.new.menu;
        let toggle = |menu: Menu| -> Pick {
            Rc::new(move |this: &mut MainView, cx: &mut Context<MainView>| {
                this.new.menu = if this.new.menu == Some(menu) {
                    None
                } else {
                    Some(menu)
                };
                cx.notify();
            })
        };
        let chips = vec![
            chip(
                "new-agent",
                agent.tag(),
                agent_color,
                agent.to_string(),
                "Agente",
                open == Some(Menu::Agent),
                toggle(Menu::Agent),
                cx,
            ),
            chip(
                "new-model",
                "",
                theme::MUTE,
                chosen.model.clone(),
                "Modelo",
                open == Some(Menu::Model),
                toggle(Menu::Model),
                cx,
            ),
            chip(
                "new-effort",
                "",
                theme::MUTE,
                chosen.effort.clone(),
                "Esfuerzo",
                open == Some(Menu::Effort),
                toggle(Menu::Effort),
                cx,
            ),
            chip(
                "new-permission",
                "",
                theme::MUTE,
                chosen.permission.clone(),
                permission_title(agent),
                open == Some(Menu::Permission),
                toggle(Menu::Permission),
                cx,
            ),
            chip(
                "new-project",
                "[#]",
                theme::MUTE,
                project.clone(),
                "Proyecto",
                open == Some(Menu::Project),
                toggle(Menu::Project),
                cx,
            ),
        ];
        let menu = open.map(|menu| {
            let (title, items) = self.new_menu_items(menu);
            menu_list(title, items, cx)
        });
        let disabled = empty || self.new.launching || command.is_none();
        let launch = div()
            .id("launch")
            .role(Role::Button)
            .aria_label("lanzar")
            .ml_auto()
            .flex()
            .items_center()
            .min_h(px(32.))
            .px(px(16.))
            .border_1()
            .border_color(color(if disabled {
                theme::HAIRLINE_STRONG
            } else {
                theme::ACCENT
            }))
            .rounded(px(4.))
            .bg(color(theme::SURFACE_ELEVATED))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(color(if disabled { theme::STONE } else { theme::INK }))
            .when(!disabled, |d| d.cursor_pointer())
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.launch(cx)))
            .child(if self.new.launching {
                "[>] lanzando…"
            } else {
                "[ok] lanzar"
            });

        div()
            .flex()
            .flex_col()
            .items_center()
            .size_full()
            .p(px(16.))
            .bg(color(theme::TERMINAL_BG))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_grow_1()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .py(px(48.))
                    .child(
                        div()
                            .text_color(color(theme::INK))
                            .text_size(px(28.))
                            .font_weight(FontWeight::BOLD)
                            .child("Nueva sesión"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .text_size(px(13.))
                            .text_color(color(theme::MUTE))
                            .child("en")
                            .child(
                                div()
                                    .text_color(color(theme::BODY))
                                    .child(format!("[#] {project}")),
                            )
                            .child("con")
                            .child(
                                div()
                                    .text_color(color(agent_color))
                                    .child(agent.to_string()),
                            ),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(820.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .children(menu)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .border_1()
                            .border_color(color(theme::ACCENT))
                            .rounded(px(4.))
                            .bg(color(theme::CANVAS))
                            .child(self.new.brief.clone())
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(4.))
                                    .p(px(8.))
                                    .border_t_1()
                                    .border_color(hairline())
                                    .children(chips)
                                    .child(launch),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.))
                            .px(px(4.))
                            .text_size(px(11.))
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(color(theme::STONE))
                                    .child("se ejecutará"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_wrap()
                                    .gap_x(px(6.6))
                                    .text_color(color(theme::MUTE))
                                    .children(
                                        command_groups(&command.unwrap_or_else(|| {
                                            "elige la carpeta de trabajo con [#] o [d]".into()
                                        }))
                                        .into_iter()
                                        .map(|group| div().min_w(px(0.)).child(group)),
                                    ),
                            ),
                    )
                    .when_some(self.new.error.clone(), |d, error| {
                        d.child(
                            div()
                                .px(px(4.))
                                .text_size(px(11.))
                                .text_color(color(theme::DANGER))
                                .child(format!("[x] {error}")),
                        )
                    }),
            )
    }

    /// Title and options of a menu of the "new session" box.
    fn new_menu_items(&self, menu: Menu) -> (&'static str, Vec<MenuItem>) {
        let agent = self.new.form.agent;
        let chosen = self.new.form.get(agent).clone();
        let set = |menu: Menu, value: String| -> Pick {
            Rc::new(move |this: &mut MainView, cx: &mut Context<MainView>| {
                let agent = this.new.form.agent;
                let form = this.new.form.get_mut(agent);
                match menu {
                    Menu::Model => form.model = value.clone(),
                    Menu::Effort => form.effort = value.clone(),
                    Menu::Permission => form.permission = value.clone(),
                    Menu::Agent | Menu::Project => {}
                }
                this.new.menu = None;
                cx.notify();
            })
        };
        match menu {
            Menu::Agent => (
                "agente",
                AgentCli::ALL
                    .into_iter()
                    .map(|a| MenuItem {
                        tag: a.tag().into(),
                        tag_color: theme::agent_color(a),
                        label: a.to_string(),
                        hint: match a {
                            AgentCli::ClaudeCode => "claude --bg".into(),
                            AgentCli::Codex => "TUI de codex".into(),
                        },
                        selected: a == agent,
                        pick: Rc::new(move |this: &mut MainView, cx: &mut Context<MainView>| {
                            this.new.form.agent = a;
                            this.new.menu = None;
                            cx.notify();
                        }),
                    })
                    .collect(),
            ),
            Menu::Model => (
                "modelo",
                with_current(models(agent), &chosen.model)
                    .into_iter()
                    .map(|m| plain_item(&m, m == chosen.model, set(Menu::Model, m.clone())))
                    .collect(),
            ),
            Menu::Effort => (
                "esfuerzo",
                efforts(agent)
                    .iter()
                    .map(|e| plain_item(e, *e == chosen.effort, set(Menu::Effort, e.to_string())))
                    .collect(),
            ),
            Menu::Permission => (
                permission_title(agent),
                permissions(agent)
                    .iter()
                    .map(|p| {
                        plain_item(
                            p,
                            *p == chosen.permission,
                            set(Menu::Permission, p.to_string()),
                        )
                    })
                    .collect(),
            ),
            Menu::Project => {
                let current = self.folder();
                let mut items: Vec<MenuItem> = self
                    .folder_choices()
                    .into_iter()
                    .map(|(root, name)| MenuItem {
                        tag: "[#]".into(),
                        tag_color: theme::MUTE,
                        label: name,
                        hint: tilde(&root, self.home.as_deref()),
                        selected: current.as_deref() == Some(root.as_str()),
                        pick: Rc::new(move |this: &mut MainView, cx: &mut Context<MainView>| {
                            this.new.folder = Some(root.clone());
                            this.new.menu = None;
                            cx.notify();
                        }),
                    })
                    .collect();
                items.push(MenuItem {
                    tag: "[d]".into(),
                    tag_color: theme::ACCENT,
                    label: "elegir otra carpeta…".into(),
                    hint: String::new(),
                    selected: false,
                    pick: Rc::new(|this: &mut MainView, cx: &mut Context<MainView>| {
                        this.pick_folder(cx)
                    }),
                });
                ("proyecto (carpeta de trabajo)", items)
            }
        }
    }

    /// `[*]`: the defaults of the "new session" box.
    fn settings_view(&self, cx: &mut Context<Self>) -> Div {
        let open = self.settings_menu;
        let toggle = |agent: AgentCli, menu: Menu| -> Pick {
            Rc::new(move |this: &mut MainView, cx: &mut Context<MainView>| {
                this.settings_menu = if this.settings_menu == Some((agent, menu)) {
                    None
                } else {
                    Some((agent, menu))
                };
                cx.notify();
            })
        };
        let pick = |agent: AgentCli, menu: Menu, value: String| -> Pick {
            Rc::new(move |this: &mut MainView, cx: &mut Context<MainView>| {
                this.set_default(agent, menu, &value);
                cx.notify();
            })
        };
        let label = |text: String, text_color: u32| {
            div()
                .w(px(160.))
                .flex_none()
                .text_size(px(12.))
                .text_color(color(text_color))
                .child(text)
        };
        let default_agent = self.defaults.agent;
        let mut rows = vec![
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(label("agente por defecto".into(), theme::MUTE))
                .child(chip(
                    "default-agent",
                    default_agent.tag(),
                    theme::agent_color(default_agent),
                    default_agent.to_string(),
                    "Agente por defecto",
                    open.is_some_and(|(_, m)| m == Menu::Agent),
                    toggle(default_agent, Menu::Agent),
                    cx,
                ))
                .into_any_element(),
        ];
        if let Some((_, Menu::Agent)) = open {
            let items = AgentCli::ALL
                .into_iter()
                .map(|a| MenuItem {
                    tag: a.tag().into(),
                    tag_color: theme::agent_color(a),
                    label: a.to_string(),
                    hint: String::new(),
                    selected: a == default_agent,
                    pick: pick(a, Menu::Agent, a.tag().to_string()),
                })
                .collect();
            rows.push(menu_list("agente por defecto", items, cx).into_any_element());
        }
        for agent in AgentCli::ALL {
            let values = self.defaults.get(agent).clone();
            let ids: [&'static str; 3] = match agent {
                AgentCli::ClaudeCode => ["cl-model", "cl-effort", "cl-permission"],
                AgentCli::Codex => ["cx-model", "cx-effort", "cx-permission"],
            };
            rows.push(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .w(px(160.))
                            .flex_none()
                            .flex()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .child(
                                div()
                                    .text_color(color(theme::agent_color(agent)))
                                    .child(agent.tag()),
                            )
                            .child(
                                div()
                                    .text_color(color(theme::MUTE))
                                    .child(agent.to_string()),
                            ),
                    )
                    .child(chip(
                        ids[0],
                        "",
                        theme::MUTE,
                        values.model.clone(),
                        "Modelo",
                        open == Some((agent, Menu::Model)),
                        toggle(agent, Menu::Model),
                        cx,
                    ))
                    .child(chip(
                        ids[1],
                        "",
                        theme::MUTE,
                        values.effort.clone(),
                        "Esfuerzo",
                        open == Some((agent, Menu::Effort)),
                        toggle(agent, Menu::Effort),
                        cx,
                    ))
                    .child(chip(
                        ids[2],
                        "",
                        theme::MUTE,
                        values.permission.clone(),
                        permission_title(agent),
                        open == Some((agent, Menu::Permission)),
                        toggle(agent, Menu::Permission),
                        cx,
                    ))
                    .into_any_element(),
            );
            if let Some((menu_agent, menu)) = open
                && menu_agent == agent
                && menu != Menu::Agent
            {
                let (title, current, options): (&str, &str, Vec<String>) = match menu {
                    Menu::Model => (
                        "modelo",
                        &values.model,
                        with_current(models(agent), &values.model),
                    ),
                    Menu::Effort => (
                        "esfuerzo",
                        &values.effort,
                        efforts(agent).iter().map(|e| e.to_string()).collect(),
                    ),
                    _ => (
                        permission_title(agent),
                        &values.permission,
                        permissions(agent).iter().map(|p| p.to_string()).collect(),
                    ),
                };
                let items = options
                    .into_iter()
                    .map(|v| plain_item(&v, v == current, pick(agent, menu, v.clone())))
                    .collect();
                rows.push(menu_list(title, items, cx).into_any_element());
            }
        }
        div()
            .flex()
            .flex_col()
            .items_center()
            .size_full()
            .p(px(16.))
            .bg(color(theme::TERMINAL_BG))
            .child(
                div()
                    .w_full()
                    .max_w(px(820.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .pt(px(48.))
                    .child(
                        div()
                            .text_color(color(theme::INK))
                            .text_size(px(28.))
                            .font_weight(FontWeight::BOLD)
                            .child("Configuración"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(color(theme::MUTE))
                            .child("Valores con los que empieza el cuadro de Nueva sesión. Se guardan al elegirlos."),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .p(px(12.))
                            .border_1()
                            .border_color(hairline())
                            .rounded(px(4.))
                            .bg(color(theme::CANVAS))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(color(theme::INK))
                                    .child("NUEVA SESIÓN · VALORES POR DEFECTO"),
                            )
                            .children(rows),
                    ),
            )
    }
}

/// What a menu option does when picked.
type Pick = Rc<dyn Fn(&mut MainView, &mut Context<MainView>)>;

struct MenuItem {
    tag: String,
    tag_color: u32,
    label: String,
    hint: String,
    selected: bool,
    pick: Pick,
}

fn plain_item(label: &str, selected: bool, pick: Pick) -> MenuItem {
    MenuItem {
        tag: String::new(),
        tag_color: theme::MUTE,
        label: label.to_string(),
        hint: String::new(),
        selected,
        pick,
    }
}

/// The offered options plus the current value when it is not one of them
/// (a model set by hand in the defaults file).
fn with_current(options: &[&str], current: &str) -> Vec<String> {
    let mut list: Vec<String> = options.iter().map(|o| o.to_string()).collect();
    if !list.iter().any(|o| o == current) {
        list.push(current.to_string());
    }
    list
}

fn permission_title(agent: AgentCli) -> &'static str {
    match agent {
        AgentCli::ClaudeCode => "modo de permisos",
        AgentCli::Codex => "sandbox",
    }
}

/// A setting button: `tag label [v]`, with an `accent` border while its
/// list is open.
#[allow(clippy::too_many_arguments)]
fn chip(
    id: &'static str,
    tag: &str,
    tag_color: u32,
    label: String,
    aria: &'static str,
    open: bool,
    toggle: Pick,
    cx: &mut Context<MainView>,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(aria)
        .flex()
        .items_center()
        .gap(px(6.))
        .min_h(px(32.))
        .px(px(8.))
        .border_1()
        .border_color(if open {
            color(theme::ACCENT)
        } else {
            hairline()
        })
        .rounded(px(4.))
        .text_size(px(12.))
        .text_color(color(theme::BODY))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| toggle(this, cx)))
        .when(!tag.is_empty(), |d| {
            d.child(div().text_color(color(tag_color)).child(tag.to_string()))
        })
        .child(label)
        .child(div().text_color(color(theme::MUTE)).child("[v]"))
}

/// The short list a setting button opens.
fn menu_list(title: &'static str, items: Vec<MenuItem>, cx: &mut Context<MainView>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .p(px(8.))
        .bg(color(theme::SURFACE_ELEVATED))
        .border_1()
        .border_color(color(theme::HAIRLINE_STRONG))
        .rounded(px(4.))
        .text_size(px(12.))
        .child(
            div()
                .px(px(8.))
                .py(px(4.))
                .text_color(color(theme::MUTE))
                .child(title),
        )
        .children(items.into_iter().enumerate().map(|(i, item)| {
            let pick = item.pick.clone();
            div()
                .id(ElementId::named_usize("menu-item", i))
                .role(Role::ListBoxOption)
                .aria_label(SharedString::from(item.label.clone()))
                .flex()
                .items_center()
                .gap(px(8.))
                .min_h(px(32.))
                .px(px(8.))
                .rounded(px(4.))
                .bg(if item.selected {
                    color(theme::SURFACE)
                } else {
                    gpui::transparent_black().into()
                })
                .text_color(color(if item.selected {
                    theme::INK
                } else {
                    theme::BODY
                }))
                .cursor_pointer()
                .hover(|style| style.bg(color(theme::SURFACE)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| pick(this, cx)))
                .child(
                    div()
                        .w(px(32.))
                        .flex_none()
                        .text_color(color(theme::ACCENT))
                        .child(if item.selected { "[ok]" } else { "" }),
                )
                .when(!item.tag.is_empty(), |d| {
                    d.child(div().text_color(color(item.tag_color)).child(item.tag))
                })
                .child(div().flex_grow_1().child(item.label))
                .child(div().text_color(color(theme::MUTE)).child(item.hint))
        }))
}

/// Splits a command line for display so it wraps before an option, never
/// between an option and its value: a group starts at every word that
/// begins with `-` or is `&&`, outside quotes. Joined with single spaces
/// the groups are the line again.
fn command_groups(line: &str) -> Vec<String> {
    let mut groups: Vec<String> = Vec::new();
    let mut quoted = false;
    let mut word_start = true;
    let mut current = String::new();
    for c in line.chars() {
        if c == '\'' {
            quoted = !quoted;
        }
        if !quoted && c == ' ' {
            word_start = true;
            current.push(c);
            continue;
        }
        if word_start && !quoted && (c == '-' || c == '&') && !current.trim().is_empty() {
            groups.push(current.trim_end().to_string());
            current.clear();
        }
        word_start = false;
        current.push(c);
    }
    if !current.trim().is_empty() {
        groups.push(current.trim_end().to_string());
    }
    groups
}

/// A launch token: unique per click, so the daemon refuses a request sent
/// twice.
fn new_token() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos:x}-{n}", std::process::id())
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

// ----- small pieces ---------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::command_groups;

    #[test]
    fn command_groups_keep_options_with_their_values() {
        let line = "cd /srv/demo/kite && claude --bg -n 'kite: a -b c' --model opus -- 'x --y'";
        let groups = command_groups(line);
        assert_eq!(
            groups,
            [
                "cd /srv/demo/kite",
                "&& claude",
                "--bg",
                "-n 'kite: a -b c'",
                "--model opus",
                "-- 'x --y'"
            ]
        );
        assert_eq!(groups.join(" "), line);
    }
}
