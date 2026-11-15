//! `[*]` configuración: the default choices of «Nueva sesión», saved in
//! titan-agent's own settings file (never in a CLI's files).
//!
//! File: `TITAN_AGENT_CONFIG`, or `$XDG_CONFIG_HOME/titan-agent/settings.json`
//! (`~/.config/…` without it, `%APPDATA%\titan-agent\…` on Windows).

use std::path::PathBuf;

use gpui::{
    ClickEvent, Context, Div, ElementId, EventEmitter, FontWeight, Window, div, prelude::*, px,
};
use serde_json::Value;
use titan_agent_core::launch::{EFFORTS, LaunchDefaults, access_modes, models};
use titan_agent_core::project::tilde;
use titan_agent_core::session::AgentCli;

use crate::new_session::{chip, menu_list, menu_option};
use crate::theme::{self, color, hairline};

pub fn settings_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("TITAN_AGENT_CONFIG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("titan-agent").join("settings.json"))
}

/// Reads the saved defaults; a missing or broken file gives the built-in
/// ones.
pub fn load_defaults() -> LaunchDefaults {
    settings_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("new_session").cloned())
        .map(|value| LaunchDefaults::parse(&value.to_string()))
        .unwrap_or_default()
}

/// Saves the defaults, keeping any other key of the file.
pub fn save_defaults(defaults: &LaunchDefaults) -> Result<(), String> {
    let path = settings_path().ok_or("no hay carpeta de configuración")?;
    let mut value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    value["new_session"] = serde_json::to_value(defaults).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|e| e.to_string())
}

pub enum SettingsEvent {
    Changed(LaunchDefaults),
}

/// One option of a list: value, tag, tag colour and label.
type Choice = (String, &'static str, u32, String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Agent,
    Model(AgentCli),
    Effort(AgentCli),
    Access(AgentCli),
}

pub struct SettingsView {
    defaults: LaunchDefaults,
    menu: Option<Field>,
    error: Option<String>,
    home: Option<String>,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl SettingsView {
    pub fn new(defaults: LaunchDefaults, home: Option<String>) -> SettingsView {
        SettingsView {
            defaults,
            menu: None,
            error: None,
            home,
        }
    }

    pub fn close_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if self.menu.take().is_some() {
            cx.notify();
            true
        } else {
            false
        }
    }

    fn toggle(&mut self, field: Field, cx: &mut Context<Self>) {
        self.menu = if self.menu == Some(field) {
            None
        } else {
            Some(field)
        };
        cx.notify();
    }

    fn set(&mut self, field: Field, value: String, cx: &mut Context<Self>) {
        match field {
            Field::Agent => {
                self.defaults.agent = if value == AgentCli::Codex.tag() {
                    AgentCli::Codex
                } else {
                    AgentCli::ClaudeCode
                }
            }
            Field::Model(agent) => self.defaults.of_mut(agent).model = value,
            Field::Effort(agent) => self.defaults.of_mut(agent).effort = value,
            Field::Access(agent) => self.defaults.of_mut(agent).access = value,
        }
        self.menu = None;
        self.error = save_defaults(&self.defaults)
            .err()
            .map(|error| format!("no se pudo guardar: {error}"));
        cx.emit(SettingsEvent::Changed(self.defaults.clone()));
        cx.notify();
    }

    fn list(&self, field: Field, cx: &mut Context<Self>) -> Div {
        let (title, values, current): (&str, Vec<Choice>, String) = match field {
            Field::Agent => (
                "agente por defecto",
                AgentCli::ALL
                    .iter()
                    .map(|a| {
                        (
                            a.tag().to_string(),
                            a.tag(),
                            theme::agent_color(*a),
                            a.to_string(),
                        )
                    })
                    .collect(),
                self.defaults.agent.tag().to_string(),
            ),
            Field::Model(agent) => (
                "modelo",
                plain(models(agent)),
                self.defaults.of(agent).model.clone(),
            ),
            Field::Effort(agent) => (
                "esfuerzo",
                plain(&EFFORTS),
                self.defaults.of(agent).effort.clone(),
            ),
            Field::Access(agent) => (
                match agent {
                    AgentCli::ClaudeCode => "modo de permisos",
                    AgentCli::Codex => "sandbox",
                },
                plain(access_modes(agent)),
                self.defaults.of(agent).access.clone(),
            ),
        };
        let mut list = menu_list(title);
        for (value, tag, tag_color, label) in values {
            let id = ElementId::Name(format!("set-{field:?}-{value}").into());
            let selected = value == current;
            list = list.child(
                menu_option(id, selected, tag, tag_color, label, String::new()).on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set(field, value.clone(), cx)
                    }),
                ),
            );
        }
        list
    }

    fn row(&self, label: &str, chips: Vec<gpui::AnyElement>) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(
                div()
                    .w(px(120.))
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(color(theme::MUTE))
                    .child(label.to_string()),
            )
            .child(div().flex().flex_wrap().gap(px(4.)).children(chips))
    }
}

fn plain(values: &[&str]) -> Vec<Choice> {
    values
        .iter()
        .map(|v| (v.to_string(), "", theme::MUTE, v.to_string()))
        .collect()
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.menu;
        let agent = self.defaults.agent;
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .w_full()
            .max_w(px(820.))
            .child(
                div()
                    .text_color(color(theme::INK))
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(13.))
                    .child("Nueva sesión"),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(color(theme::MUTE))
                    .child(
                        "Valores con los que se abre el cuadro de nueva sesión. \
                         Se pueden cambiar en cada lanzamiento.",
                    ),
            );

        let agent_chip = chip(
            "set-agent",
            agent.tag(),
            theme::agent_color(agent),
            agent.to_string(),
            open == Some(Field::Agent),
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle(Field::Agent, cx)))
        .into_any_element();
        body = body.child(self.row("agente", vec![agent_chip]));
        if open == Some(Field::Agent) {
            body = body.child(self.list(Field::Agent, cx));
        }

        for agent in AgentCli::ALL {
            let d = self.defaults.of(agent).clone();
            let chips = [
                (Field::Model(agent), d.model),
                (Field::Effort(agent), d.effort),
                (Field::Access(agent), d.access),
            ]
            .into_iter()
            .map(|(field, value)| {
                chip(
                    ElementId::Name(format!("set-chip-{field:?}").into()),
                    "",
                    theme::MUTE,
                    value,
                    open == Some(field),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle(field, cx)))
                .into_any_element()
            })
            .collect();
            body = body.child(self.row(&agent.to_string(), chips));
            if let Some(field) = open.filter(|f| {
                matches!(f, Field::Model(a) | Field::Effort(a) | Field::Access(a) if *a == agent)
            }) {
                body = body.child(self.list(field, cx));
            }
        }

        let path = settings_path()
            .map(|p| tilde(&p.to_string_lossy(), self.home.as_deref()))
            .unwrap_or_else(|| "sin carpeta de configuración".into());
        body = body.child(
            div()
                .pt(px(4.))
                .text_size(px(11.))
                .text_color(color(theme::STONE))
                .child(format!("se guardan en {path}")),
        );
        if let Some(error) = self.error.clone() {
            body = body.child(
                div()
                    .flex()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .child(div().text_color(color(theme::DANGER)).child("[x]"))
                    .child(error),
            );
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
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
                            .child("[*] Configuración"),
                    )
                    .child("valores por defecto"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .flex_grow_1()
                    .p(px(24.))
                    .bg(color(theme::TERMINAL_BG))
                    .child(body),
            )
    }
}
