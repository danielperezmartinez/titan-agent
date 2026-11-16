//! Setting buttons and their short option lists, shared by «Nueva sesión» and
//! the settings view.

use std::rc::Rc;

use gpui::{
    ClickEvent, Context, Div, ElementId, Role, SharedString, Stateful, div, prelude::*, px,
};
use titan_agent_core::session::AgentCli;

use crate::theme::{self, color, hairline};

use super::MainView;

/// What a menu option does when picked.
pub(super) type Pick = Rc<dyn Fn(&mut MainView, &mut Context<MainView>)>;

pub(super) struct MenuItem {
    pub(super) tag: String,
    pub(super) tag_color: u32,
    pub(super) label: String,
    pub(super) hint: String,
    pub(super) selected: bool,
    pub(super) pick: Pick,
}

pub(super) fn plain_item(label: &str, selected: bool, pick: Pick) -> MenuItem {
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
pub(super) fn with_current(options: &[&str], current: &str) -> Vec<String> {
    let mut list: Vec<String> = options.iter().map(|o| o.to_string()).collect();
    if !list.iter().any(|o| o == current) {
        list.push(current.to_string());
    }
    list
}

pub(super) fn permission_title(agent: AgentCli) -> &'static str {
    match agent {
        AgentCli::ClaudeCode => "modo de permisos",
        AgentCli::Codex => "sandbox",
    }
}

/// A setting button: `tag label [v]`, with an `accent` border while its
/// list is open.
#[allow(clippy::too_many_arguments)]
pub(super) fn chip(
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
pub(super) fn menu_list(
    title: &'static str,
    items: Vec<MenuItem>,
    cx: &mut Context<MainView>,
) -> Div {
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
