//! `[*]` Configuración: the defaults of the «Nueva sesión» box.

use std::rc::Rc;

use gpui::{Context, Div, FontWeight, div, prelude::*, px};
use titan_agent_core::launch::{efforts, models, permissions};
use titan_agent_core::session::AgentCli;

use crate::theme::{self, color, hairline};

use super::MainView;
use super::menu::{MenuItem, Pick, chip, menu_list, permission_title, plain_item, with_current};
use super::new_session::Menu;

impl MainView {
    pub(super) fn set_default(&mut self, agent: AgentCli, menu: Menu, value: &str) {
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

    /// `[*]`: the defaults of the "new session" box.
    pub(super) fn settings_view(&self, cx: &mut Context<Self>) -> Div {
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
