//! The conversation view in its draft state, what was the «Nueva sesión»
//! box (docs/Decisiones visuales/Conversación de sesión y campo de texto
//! común.md): settings, the exact command and the common text field; Enter
//! launches and the view becomes the session's (ADR-0002: the brief is only
//! what the user types; one Enter, one launch; the exact command is shown
//! before launching).

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Context, Div, Entity, Focusable, FontWeight, PathPromptOptions, Role, Window,
    div, prelude::*, px, relative,
};
use titan_agent_core::launch::{
    LaunchDefaults, LaunchSpec, efforts, folder_name, models, permissions,
};
use titan_agent_core::project::{project_name, tilde};
use titan_agent_core::protocol::{LaunchRef, LaunchResult, Request};
use titan_agent_core::session::AgentCli;

use titan_agent::conversation::SessionBody;

use super::composer::Composer;
use crate::terminal::TerminalView;
use crate::theme::{self, color, hairline};

use super::menu::{
    Menu, MenuItem, Pick, chip, menu_list, permission_title, plain_item, with_current,
};
use super::{MainView, Pending, Selection};

/// Key of the draft's field in [`titan_agent::composer::Drafts`].
pub(super) const DRAFT_KEY: &str = "new";

/// State of the "new session" box (ADR-0002: the brief is only what the
/// user types; one click, one launch; the exact command is shown).
pub(super) struct NewSession {
    pub(super) brief: Entity<Composer>,
    /// Current choices, starting from the configured defaults.
    pub(super) form: LaunchDefaults,
    /// Working folder (a project root or a folder picked with `[d]`).
    pub(super) folder: Option<String>,
    pub(super) menu: Option<Menu>,
    /// A launch request is on its way: the button does nothing meanwhile.
    pub(super) launching: bool,
    /// The brief of the launch on its way, to empty the field only if it
    /// was not edited meanwhile.
    pub(super) sent: Option<String>,
    pub(super) error: Option<String>,
}

impl MainView {
    /// Shows the "new session" box, in the project of the current selection
    /// when there is one.
    pub(super) fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = match &self.selection {
            Selection::Session(key) => self.session(key).map(|s| s.project_root.clone()),
            Selection::Project(root) => Some(root.clone()),
            _ => None,
        };
        if let Some(root) = current {
            self.new.folder = Some(root);
        }
        if self.selection != Selection::New {
            self.before_draft = self.selection.clone();
        }
        self.selection = Selection::New;
        self.new.menu = None;
        let handle = self.new.brief.read(cx).focus_handle(cx);
        handle.focus(window, cx);
        cx.notify();
    }

    /// `[esc] cerrar`: back to what was selected before the draft. The
    /// text stays for the next time.
    pub(super) fn close_draft(&mut self, cx: &mut Context<Self>) {
        self.new.menu = None;
        self.selection = std::mem::replace(&mut self.before_draft, Selection::None);
        if let Selection::Session(key) = self.selection.clone() {
            self.select_session(key, None, cx);
        }
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

    /// One Enter in the field: one launch request with a fresh token.
    /// Nothing is retried.
    pub(super) fn launch(&mut self, cx: &mut Context<Self>) {
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
    pub(super) fn launched(&mut self, launched: LaunchResult, cx: &mut Context<Self>) {
        let sent = self.new.sent.take();
        if let Some(text) = &sent {
            // ↑ on the empty field brings it back.
            self.drafts.record_sent(DRAFT_KEY, text);
            let last = self.drafts.last_sent(DRAFT_KEY).map(str::to_string);
            self.new.brief.update(cx, |brief, _| brief.last_sent = last);
        }
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
            // The new session starts on its conversation, whatever the
            // previous one showed.
            self.body = SessionBody::Terminal;
            self.task_menu = false;
            self.selection = Selection::Session(key);
            self.focus_terminal = true;
            cx.notify();
        } else {
            self.select_session(key, None, cx);
        }
    }

    /// Moves what belongs to a launch (selection, terminal) to the real
    /// session once the daemon knows it.
    pub(super) fn follow_launches(&mut self, launches: &[LaunchRef], cx: &mut Context<Self>) {
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

    pub(super) fn draft_view(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let agent = self.new.form.agent;
        let chosen = self.new.form.get(agent).clone();
        let agent_color = theme::agent_color(agent);
        let folder = self.folder();
        let project = folder
            .as_deref()
            .map(project_name)
            .unwrap_or_else(|| "sin carpeta".into());
        let command = self.spec(cx).map(|mut spec| {
            // The exact command (ADR-0002), only with a marker while the
            // field is empty.
            if spec.brief.trim().is_empty() {
                spec.brief = "<tu encargo>".into();
            }
            spec.display_line(self.home.as_deref())
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
                format!("esfuerzo {}", chosen.effort),
                "Esfuerzo",
                open == Some(Menu::Effort),
                toggle(Menu::Effort),
                cx,
            ),
            chip(
                "new-permission",
                "",
                theme::MUTE,
                format!(
                    "{} {}",
                    permission_title(agent).to_lowercase(),
                    chosen.permission
                ),
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
        let bar = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(16.))
            .h(px(40.))
            .flex_none()
            .border_b_1()
            .border_color(hairline())
            .text_size(px(13.))
            .text_color(color(theme::MUTE))
            .child(div().text_color(color(theme::INK)).child("[+]"))
            .child(
                div()
                    .text_color(color(theme::INK))
                    .font_weight(FontWeight::BOLD)
                    .child("Nueva sesión"),
            )
            .child("en")
            .child(
                div()
                    .text_color(color(theme::BODY))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .min_w(px(0.))
                    .child(format!("[#] {project}")),
            )
            .child(div().flex_grow_1())
            .child(
                div()
                    .id("close-draft")
                    .role(Role::Button)
                    .aria_label("cerrar")
                    .flex_none()
                    .text_size(px(12.))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_draft(cx)))
                    .child("[esc] cerrar"),
            );
        let hint = if self.new.launching {
            "[>] lanzando…".to_string()
        } else {
            "enter lanzar · shift+enter nueva línea · «- » lista · ↑ el último encargo".into()
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(bar)
            .child(
                div()
                    .flex()
                    .flex_grow_1()
                    .min_h(px(0.))
                    .items_end()
                    .justify_center()
                    .pb(px(24.))
                    .text_size(px(12.))
                    .text_color(color(theme::MUTE))
                    .child("la conversación empieza aquí"),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .gap(px(8.))
                    .px(px(16.))
                    .pt(px(12.))
                    .pb(px(14.))
                    .border_t_1()
                    .border_color(hairline())
                    // A settings menu floats over the empty conversation,
                    // just above the chips.
                    .children(menu.map(|menu| {
                        div()
                            .absolute()
                            .bottom(relative(1.))
                            .left(px(16.))
                            .w(px(420.))
                            .pb(px(4.))
                            .child(menu)
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(4.))
                            .children(chips),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_x(px(6.6))
                            .px(px(2.))
                            .text_size(px(11.))
                            .text_color(color(theme::MUTE))
                            .child(div().flex_none().child("$"))
                            .children(
                                command_groups(&command.unwrap_or_else(|| {
                                    "elige la carpeta de trabajo con [#] o [d]".into()
                                }))
                                .into_iter()
                                .map(|group| div().min_w(px(0.)).child(group)),
                            ),
                    )
                    .child(self.new.brief.clone())
                    .child(
                        div()
                            .px(px(2.))
                            .text_size(px(11.))
                            .text_color(color(if self.new.launching {
                                theme::MUTE
                            } else {
                                theme::STONE
                            }))
                            .child(hint),
                    )
                    .when_some(self.new.error.clone(), |d, error| {
                        d.child(
                            div()
                                .px(px(2.))
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
