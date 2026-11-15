//! «Nueva sesión» (docs/Decisiones visuales/Ventana principal al estilo
//! Claude Code Desktop.md): an empty brief box with the default choices
//! already set, buttons that open a short list to change agent, model,
//! effort, permissions (or sandbox) and project, the exact command that will
//! run, and `[ok] lanzar`. Launching follows ADR-0002: the brief is the
//! user's text, and each click asks the daemon for one launch.

use gpui::{
    ClickEvent, Context, Div, ElementId, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    PathPromptOptions, Stateful, Subscription, Window, div, prelude::*, px,
};
use titan_agent_core::launch::{EFFORTS, LaunchDefaults, LaunchSpec, access_modes, models};
use titan_agent_core::project::{project_name, tilde};
use titan_agent_core::session::AgentCli;

use crate::text_area::{TextArea, TextAreaEvent};
use crate::theme::{self, color, hairline};

/// A folder the new session can work in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectChoice {
    pub root: String,
    pub name: String,
}

impl ProjectChoice {
    pub fn from_root(root: &str) -> ProjectChoice {
        ProjectChoice {
            root: root.to_string(),
            name: project_name(root),
        }
    }
}

/// The list opened by one of the buttons under the brief.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    Agent,
    Model,
    Effort,
    Access,
    Project,
}

/// A launch the user asked for with one click.
pub struct LaunchRequest {
    pub spec: LaunchSpec,
    pub shown: String,
    pub click: String,
}

pub enum NewSessionEvent {
    Launch(LaunchRequest),
}

pub struct NewSessionView {
    brief: Entity<TextArea>,
    agent: AgentCli,
    /// Model, effort and access per agent (index 0 Claude Code, 1 Codex).
    choices: [(String, String, String); 2],
    project: Option<ProjectChoice>,
    projects: Vec<ProjectChoice>,
    /// Folders picked with `[d] elegir otra carpeta…`.
    picked: Vec<ProjectChoice>,
    menu: Option<Menu>,
    launching: bool,
    error: Option<String>,
    home: Option<String>,
    clicks: u64,
    _brief_events: Subscription,
}

impl EventEmitter<NewSessionEvent> for NewSessionView {}

impl Focusable for NewSessionView {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.brief.read(cx).focus_handle(cx)
    }
}

fn slot(agent: AgentCli) -> usize {
    match agent {
        AgentCli::ClaudeCode => 0,
        AgentCli::Codex => 1,
    }
}

impl NewSessionView {
    pub fn new(defaults: &LaunchDefaults, home: Option<String>, cx: &mut Context<Self>) -> Self {
        let brief = cx.new(|cx| {
            TextArea::new(
                "Escribe el encargo. titan-agent no lo completa ni lo reescribe.",
                4,
                14,
                cx,
            )
        });
        let subscription = cx.subscribe(&brief, |this, _, event, cx| match event {
            TextAreaEvent::Changed => {
                this.error = None;
                cx.notify();
            }
            TextAreaEvent::Submit => this.launch(cx),
        });
        let mut view = NewSessionView {
            brief,
            agent: defaults.agent,
            choices: Default::default(),
            project: None,
            projects: Vec::new(),
            picked: Vec::new(),
            // Opens a list at start, like TITAN_AGENT_NEW (for screenshots).
            menu: match std::env::var("TITAN_AGENT_MENU").as_deref() {
                Ok("agent") => Some(Menu::Agent),
                Ok("model") => Some(Menu::Model),
                Ok("effort") => Some(Menu::Effort),
                Ok("access") => Some(Menu::Access),
                Ok("project") => Some(Menu::Project),
                _ => None,
            },
            launching: false,
            error: None,
            home,
            clicks: 0,
            _brief_events: subscription,
        };
        view.apply_defaults(defaults);
        // Preselects a folder, like TITAN_AGENT_NEW selects the box.
        if let Ok(folder) = std::env::var("TITAN_AGENT_FOLDER")
            && !folder.is_empty()
        {
            let choice = ProjectChoice::from_root(&folder);
            view.picked.push(choice.clone());
            view.project = Some(choice);
        }
        view
    }

    /// Puts the default choices back (when the box opens with nothing typed,
    /// or after the defaults change in `[*]`).
    pub fn apply_defaults(&mut self, defaults: &LaunchDefaults) {
        self.agent = defaults.agent;
        for agent in AgentCli::ALL {
            let d = defaults.of(agent);
            self.choices[slot(agent)] = (d.model.clone(), d.effort.clone(), d.access.clone());
        }
    }

    /// Updates the projects offered, in tree order, and picks `preferred`
    /// (the project of the current selection) when the box opens with
    /// nothing typed yet.
    pub fn set_projects(
        &mut self,
        projects: Vec<ProjectChoice>,
        preferred: Option<String>,
        cx: &gpui::App,
    ) {
        self.projects = projects;
        if let Some(root) = preferred
            && self.brief.read(cx).text().is_empty()
            && !self.launching
        {
            self.project = Some(
                self.all_projects()
                    .into_iter()
                    .find(|p| p.root == root)
                    .unwrap_or_else(|| ProjectChoice::from_root(&root)),
            );
        }
        if self.project.is_none() {
            self.project = self.all_projects().into_iter().next();
        }
    }

    /// Updates the offered projects without changing the chosen one.
    pub fn refresh_projects(&mut self, projects: Vec<ProjectChoice>) {
        self.projects = projects;
        if self.project.is_none() {
            self.project = self.all_projects().into_iter().next();
        }
    }

    fn all_projects(&self) -> Vec<ProjectChoice> {
        let mut all = self.picked.clone();
        for project in &self.projects {
            if !all.iter().any(|p| p.root == project.root) {
                all.push(project.clone());
            }
        }
        all
    }

    pub fn close_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if self.menu.take().is_some() {
            cx.notify();
            true
        } else {
            false
        }
    }

    /// The daemon accepted the launch: clear the brief for the next one.
    pub fn launched(&mut self, cx: &mut Context<Self>) {
        self.launching = false;
        self.error = None;
        self.brief.update(cx, |brief, cx| brief.clear(cx));
        cx.notify();
    }

    /// The daemon refused or failed the launch. Nothing is retried.
    pub fn failed(&mut self, message: String, cx: &mut Context<Self>) {
        self.launching = false;
        self.error = Some(message);
        cx.notify();
    }

    fn spec(&self, cx: &gpui::App) -> Option<LaunchSpec> {
        let (model, effort, access) = self.choices[slot(self.agent)].clone();
        Some(LaunchSpec {
            agent: self.agent,
            model,
            effort,
            access,
            cwd: self.project.as_ref()?.root.clone(),
            brief: self.brief.read(cx).text().to_string(),
        })
    }

    fn launch(&mut self, cx: &mut Context<Self>) {
        if self.launching {
            return;
        }
        let Some(spec) = self.spec(cx) else {
            self.error = Some("elige un proyecto o una carpeta".into());
            cx.notify();
            return;
        };
        if let Err(error) = spec.validate() {
            self.error = Some(error.message());
            cx.notify();
            return;
        }
        self.clicks += 1;
        let click = format!(
            "{}-{}-{}",
            std::process::id(),
            crate::app::now_ms(),
            self.clicks
        );
        let shown = spec.command_line(self.home.as_deref());
        self.launching = true;
        self.menu = None;
        self.error = None;
        cx.emit(NewSessionEvent::Launch(LaunchRequest {
            spec,
            shown,
            click,
        }));
        cx.notify();
    }

    fn toggle(&mut self, menu: Menu, cx: &mut Context<Self>) {
        self.menu = if self.menu == Some(menu) {
            None
        } else {
            Some(menu)
        };
        cx.notify();
    }

    fn pick_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Elegir carpeta".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let root = path.to_string_lossy().into_owned();
            let _ = this.update(cx, |this, cx| {
                let choice = ProjectChoice::from_root(&root);
                this.picked.retain(|p| p.root != choice.root);
                this.picked.insert(0, choice.clone());
                this.project = Some(choice);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    // ----- rendering -------------------------------------------------------

    fn menu_view(&self, menu: Menu, cx: &mut Context<Self>) -> Div {
        let agent = self.agent;
        let (model, effort, access) = self.choices[slot(agent)].clone();
        let (title, current): (&str, String) = match menu {
            Menu::Agent => ("agente", agent.tag().to_string()),
            Menu::Model => ("modelo", model),
            Menu::Effort => ("esfuerzo", effort),
            Menu::Access => (
                match agent {
                    AgentCli::ClaudeCode => "modo de permisos",
                    AgentCli::Codex => "sandbox",
                },
                access,
            ),
            Menu::Project => (
                "proyecto (carpeta de trabajo)",
                self.project
                    .as_ref()
                    .map(|p| p.root.clone())
                    .unwrap_or_default(),
            ),
        };
        let mut list = menu_list(title);
        match menu {
            Menu::Agent => {
                for (option, hint) in [
                    (AgentCli::ClaudeCode, "claude --bg"),
                    (AgentCli::Codex, "TUI de codex"),
                ] {
                    list = list.child(
                        menu_option(
                            ElementId::Name(format!("agent-{}", option.tag()).into()),
                            current == option.tag(),
                            option.tag(),
                            theme::agent_color(option),
                            option.to_string(),
                            hint.to_string(),
                        )
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                this.agent = option;
                                this.menu = None;
                                cx.notify();
                            },
                        )),
                    );
                }
            }
            Menu::Model | Menu::Effort | Menu::Access => {
                let values: &[&str] = match menu {
                    Menu::Model => models(agent),
                    Menu::Effort => &EFFORTS,
                    _ => access_modes(agent),
                };
                for value in values {
                    let value = value.to_string();
                    let id = ElementId::Name(format!("{menu:?}-{value}").into());
                    let hint = option_hint(agent, menu, &value);
                    list = list.child(
                        menu_option(id, current == value, "", theme::MUTE, value.clone(), hint)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                let choice = &mut this.choices[slot(this.agent)];
                                match menu {
                                    Menu::Model => choice.0 = value.clone(),
                                    Menu::Effort => choice.1 = value.clone(),
                                    _ => choice.2 = value.clone(),
                                }
                                this.menu = None;
                                cx.notify();
                            })),
                    );
                }
            }
            Menu::Project => {
                for project in self.all_projects() {
                    let id = ElementId::Name(format!("project-{}", project.root).into());
                    let hint = tilde(&project.root, self.home.as_deref());
                    let selected = current == project.root;
                    list = list.child(
                        menu_option(id, selected, "[#]", theme::MUTE, project.name.clone(), hint)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.project = Some(project.clone());
                                this.menu = None;
                                cx.notify();
                            })),
                    );
                }
                list = list.child(
                    menu_option(
                        "project-other".into(),
                        false,
                        "[d]",
                        theme::ACCENT,
                        "elegir otra carpeta…".into(),
                        String::new(),
                    )
                    .on_click(cx.listener(
                        |this, _: &ClickEvent, window, cx| {
                            this.pick_folder(window, cx);
                        },
                    )),
                );
            }
        }
        list
    }
}

/// Short hint next to some options, so the list explains itself.
fn option_hint(agent: AgentCli, menu: Menu, value: &str) -> String {
    match (agent, menu, value) {
        (AgentCli::ClaudeCode, Menu::Access, "manual") => "pide permiso para cada cambio",
        (AgentCli::ClaudeCode, Menu::Access, "plan") => "solo lee y propone un plan",
        (AgentCli::ClaudeCode, Menu::Access, "acceptEdits") => "acepta las ediciones",
        (AgentCli::ClaudeCode, Menu::Access, "auto") => "decide el clasificador de Claude Code",
        (AgentCli::Codex, Menu::Access, "read-only") => "solo lectura",
        (AgentCli::Codex, Menu::Access, "workspace-write") => "escribe en la carpeta de trabajo",
        _ => "",
    }
    .to_string()
}

impl Render for NewSessionView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let agent = self.agent;
        let agent_color = theme::agent_color(agent);
        let (model, effort, access) = self.choices[slot(agent)].clone();
        let project_name = self
            .project
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "elige proyecto".into());
        let spec = self.spec(cx);
        let command = spec
            .as_ref()
            .map(|spec| spec.command_preview(self.home.as_deref()))
            .unwrap_or_else(|| "elige un proyecto o una carpeta".into());
        let ready = spec.as_ref().is_some_and(|spec| spec.validate().is_ok()) && !self.launching;
        let open = self.menu;

        let chips = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.))
            .p(px(8.))
            .border_t_1()
            .border_color(hairline())
            .child(
                chip(
                    "chip-agent",
                    agent.tag(),
                    agent_color,
                    agent.to_string(),
                    open == Some(Menu::Agent),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle(Menu::Agent, cx))),
            )
            .child(
                chip(
                    "chip-model",
                    "",
                    theme::MUTE,
                    model,
                    open == Some(Menu::Model),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle(Menu::Model, cx))),
            )
            .child(
                chip(
                    "chip-effort",
                    "",
                    theme::MUTE,
                    effort,
                    open == Some(Menu::Effort),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle(Menu::Effort, cx))),
            )
            .child(
                chip(
                    "chip-access",
                    "",
                    theme::MUTE,
                    access,
                    open == Some(Menu::Access),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle(Menu::Access, cx))),
            )
            .child(
                chip(
                    "chip-project",
                    "[#]",
                    theme::MUTE,
                    project_name.clone(),
                    open == Some(Menu::Project),
                )
                .on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| this.toggle(Menu::Project, cx)),
                ),
            )
            .child(div().flex_grow_1())
            .child(
                div()
                    .id("launch")
                    .flex()
                    .items_center()
                    .min_h(px(32.))
                    .px(px(16.))
                    .border_1()
                    .border_color(color(if ready {
                        theme::ACCENT
                    } else {
                        theme::HAIRLINE_STRONG
                    }))
                    .rounded(px(4.))
                    .bg(color(theme::SURFACE_ELEVATED))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color(if ready { theme::INK } else { theme::STONE }))
                    .when(ready, |d| d.cursor_pointer())
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.launch(cx)))
                    .child(if self.launching {
                        "[>] lanzando…"
                    } else {
                        "[ok] lanzar"
                    }),
            );

        let box_ = div()
            .flex()
            .flex_col()
            .border_1()
            .border_color(color(theme::ACCENT))
            .rounded(px(4.))
            .bg(color(theme::CANVAS))
            .child(self.brief.clone())
            .child(chips);

        div()
            .id("new-session-view")
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
                                    .child(format!("[#] {project_name}")),
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
                    .when_some(open, |d, menu| d.child(self.menu_view(menu, cx)))
                    .child(box_)
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
                                    .text_color(color(theme::MUTE))
                                    .child(command),
                            ),
                    )
                    .when_some(self.error.clone(), |d, error| {
                        d.child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .px(px(4.))
                                .text_size(px(12.))
                                .text_color(color(theme::BODY))
                                .child(div().text_color(color(theme::DANGER)).child("[x]"))
                                .child(error),
                        )
                    }),
            )
    }
}

// ----- shared pieces (also used by the settings view) -----------------------

/// A button under the brief that opens a short list.
pub fn chip(
    id: impl Into<ElementId>,
    tag: &str,
    tag_color: u32,
    label: String,
    open: bool,
) -> Stateful<Div> {
    div()
        .id(id.into())
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
        .when(!tag.is_empty(), |d| {
            d.child(div().text_color(color(tag_color)).child(tag.to_string()))
        })
        .child(label)
        .child(div().text_color(color(theme::MUTE)).child("[v]"))
}

/// The open list: a titled box of options.
pub fn menu_list(title: &str) -> Div {
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
                .child(title.to_string()),
        )
}

/// One option of a list, `[ok]` when it is the current one.
pub fn menu_option(
    id: ElementId,
    selected: bool,
    tag: &str,
    tag_color: u32,
    label: String,
    hint: String,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(8.))
        .min_h(px(32.))
        .px(px(8.))
        .rounded(px(4.))
        .when(selected, |d| d.bg(color(theme::SURFACE)))
        .when(!selected, |d| {
            d.hover(|style| style.bg(color(theme::SURFACE_ELEVATED).blend(hairline())))
        })
        .text_color(color(if selected { theme::INK } else { theme::BODY }))
        .cursor_pointer()
        .child(
            div()
                .w(px(32.))
                .flex_none()
                .text_color(color(theme::ACCENT))
                .child(if selected { "[ok]" } else { "" }),
        )
        .when(!tag.is_empty(), |d| {
            d.child(div().text_color(color(tag_color)).child(tag.to_string()))
        })
        .child(div().flex_grow_1().child(label))
        .child(div().text_color(color(theme::MUTE)).child(hint))
}
