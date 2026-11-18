//! Left column: «Nueva sesión», the filter, the tree of imported projects
//! (with their `+`) and «otras carpetas» (docs/Decisiones/ADR-0014), and the
//! legend. The rows come from `titan_agent_core::tree::project_tree`.

use gpui::{
    AnyElement, ClickEvent, Context, ElementId, FontWeight, KeyDownEvent, Role, SharedString,
    Window, div, prelude::*, px,
};
use titan_agent_core::tree::{NavRow, TreeRow, build_rows, project_tree};

use crate::theme::{self, color, hairline};

use super::widgets::{agent_pill, caret, hover_enabled, marked, state_marker};
use super::{MainView, Selection};

impl MainView {
    pub(super) fn filter_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn nav(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
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

        let rows: Vec<NavRow> = match &self.projects {
            Some(projects) => project_tree(&self.sessions, projects, &self.teams, &self.filter),
            // A daemon that does not know imported projects: every session
            // grouped by its project, as before.
            None => build_rows(&self.sessions, &self.filter)
                .into_iter()
                .map(|row| match row {
                    TreeRow::Project {
                        name, root, count, ..
                    } => NavRow::Project {
                        root,
                        name,
                        count,
                        imported: true,
                        missing: false,
                    },
                    TreeRow::Session { key, depth, branch } => {
                        NavRow::Session { key, depth, branch }
                    }
                })
                .collect(),
        };
        // Why the list of projects is not there, or the last import or
        // removal refused: under the title, whatever is selected.
        let projects_problem = self
            .projects_error
            .clone()
            .map(|e| format!("no se puede leer la lista de proyectos: {e}"))
            .or_else(|| self.projects_refused.clone());
        let mut tree = div()
            .id("tree")
            .flex()
            .flex_col()
            .flex_grow_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .pb(px(12.));
        if let Some(problem) = projects_problem {
            tree = tree.child(
                div()
                    .px(px(12.))
                    .pt(px(4.))
                    .text_size(px(11.))
                    .text_color(color(theme::DANGER))
                    .child(format!("[x] {problem}")),
            );
        }
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
                        "ningún proyecto: impórtalo con [+] importar".to_string()
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
                                    .child("PROYECTOS"),
                            )
                            .when(self.projects.is_some(), |d| {
                                d.child(
                                    div()
                                        .id("import-project")
                                        .text_color(color(theme::ACCENT))
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.pick_import(cx)
                                        }))
                                        .child("[+] importar"),
                                )
                            }),
                    ),
            )
            .child(tree)
            .child(self.legend())
    }

    /// `[+] importar` of the title: the folder picker, then `project_add`.
    fn pick_import(&mut self, cx: &mut Context<Self>) {
        let answer = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Importar una carpeta como proyecto".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = answer.await
                && let Some(path) = paths.into_iter().next()
            {
                let folder = path.to_string_lossy().into_owned();
                let _ = this.update(cx, |view, cx| view.import_project(folder, cx));
            }
        })
        .detach();
    }

    /// A small line under a project: «sin sesiones · [+] nueva» or the
    /// folder that is gone, with its action.
    fn project_note(
        &self,
        id: String,
        marker: (&'static str, u32),
        text: &'static str,
        act: (&'static str, u32),
        on: impl Fn(&mut MainView, &mut Window, &mut Context<MainView>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .min_h(px(28.))
            .pl(px(24.))
            .pr(px(12.))
            .text_size(px(12.))
            .text_color(color(theme::MUTE))
            .when(!marker.0.is_empty(), |d| {
                d.child(div().text_color(color(marker.1)).child(marker.0))
            })
            .child(text)
            .child("·")
            .child(
                div()
                    .id(SharedString::from(id))
                    .text_color(color(act.1))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, window, cx| on(this, window, cx)),
                    )
                    .child(act.0),
            )
            .into_any_element()
    }

    fn tree_row(&self, row: NavRow, cx: &mut Context<Self>) -> AnyElement {
        match row {
            NavRow::Others => div()
                .pt(px(16.))
                .px(px(12.))
                .text_size(px(12.))
                .text_color(color(theme::MUTE))
                .child("OTRAS CARPETAS")
                .into_any_element(),
            NavRow::Empty { root } => {
                // A folder that is gone already says so under its header.
                if self
                    .projects
                    .iter()
                    .flatten()
                    .any(|p| p.root == root && p.missing)
                {
                    return div().into_any_element();
                }
                self.project_note(
                    format!("empty:{root}"),
                    ("", theme::MUTE),
                    "sin sesiones",
                    ("[+] nueva", theme::ACCENT),
                    move |this, window, cx| this.open_new_in(root.clone(), window, cx),
                    cx,
                )
            }
            NavRow::Project {
                name,
                root,
                imported,
                missing,
                ..
            } => {
                let selected = self.selection == Selection::Project(root.clone())
                    || (matches!(self.selection, Selection::Thread(_))
                        && self.thread.as_ref().and_then(|t| t.r.root.as_deref())
                            == Some(root.as_str()));
                let live = self
                    .sessions
                    .iter()
                    .filter(|s| s.project_root == root && s.state.is_live())
                    .count();
                let group = SharedString::from(format!("pg:{root}"));
                let plus_root = root.clone();
                let import_root = root.clone();
                let remove_root = root.clone();
                let open_root = root.clone();
                let id = ElementId::Name(SharedString::from(format!("p:{root}")));
                let header = div()
                    .id(id)
                    .group(group.clone())
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
                    .when(!selected && hover_enabled(), |d| {
                        d.hover(|style| style.bg(color(theme::CANVAS).blend(hairline())))
                    })
                    .text_size(px(12.))
                    .text_color(color(if imported { theme::MUTE } else { theme::STONE }))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.select_project(open_root.clone());
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
                            .text_color(color(match (selected, imported) {
                                (true, _) => theme::INK,
                                (false, true) => theme::BODY,
                                (false, false) => theme::MUTE,
                            }))
                            .font_weight(FontWeight::MEDIUM)
                            .child(name),
                    )
                    .child(div().flex_grow_1().min_w(px(8.)).h(px(1.)).bg(hairline()))
                    // Live sessions, as the summary counts them; a folder
                    // that is gone shows `[!]` in their place.
                    .child(if missing {
                        div()
                            .flex_none()
                            .text_color(color(theme::WARNING))
                            .child("[!]")
                    } else {
                        div().flex_none().child(live.to_string())
                    })
                    .map(|d| {
                        if imported {
                            // `+`: the draft with this project, shown on
                            // hover or with the header selected.
                            d.when(!missing, |d| {
                                d.child(
                                    div()
                                        .id(SharedString::from(format!("plus:{root}")))
                                        .flex_none()
                                        .px(px(5.))
                                        .border_1()
                                        .rounded(px(4.))
                                        .border_color(hairline())
                                        .text_size(px(12.))
                                        .text_color(color(theme::INK))
                                        .opacity(if selected { 1. } else { 0. })
                                        .when(hover_enabled(), |d| {
                                            d.group_hover(group.clone(), |s| s.opacity(1.))
                                        })
                                        .cursor_pointer()
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, window, cx| {
                                                cx.stop_propagation();
                                                this.open_new_in(plus_root.clone(), window, cx)
                                            },
                                        ))
                                        .child("+"),
                                )
                            })
                        } else {
                            d.child(
                                div()
                                    .id(SharedString::from(format!("import:{root}")))
                                    .flex_none()
                                    .text_color(color(theme::ACCENT))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        cx.stop_propagation();
                                        this.import_project(import_root.clone(), cx)
                                    }))
                                    .child("[+] importar"),
                            )
                        }
                    });
                if !missing {
                    return header.into_any_element();
                }
                div()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(self.project_note(
                        format!("gone:{root}"),
                        ("[!]", theme::WARNING),
                        "carpeta no encontrada",
                        ("[x] quitar", theme::DANGER),
                        move |this, _, cx| this.remove_project(remove_root.clone(), cx),
                        cx,
                    ))
                    .into_any_element()
            }
            NavRow::Session { key, depth, branch } => {
                let Some(session) = self.session(&key) else {
                    return div().into_any_element();
                };
                let selected = self.selection == Selection::Session(key.clone());
                let finished = !session.state.is_live();
                let director = !self.director_tasks(&key).is_empty();
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
                    .when(!selected && hover_enabled(), |d| {
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
                            .flex_shrink(1.)
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(color(if finished { theme::MUTE } else { theme::BODY }))
                            .child(session.name.clone()),
                    )
                    .when(director, |d| {
                        d.child(
                            div()
                                .flex_none()
                                .text_color(color(theme::SUCCESS))
                                .child("[*]"),
                        )
                    })
                    .child(div().flex_grow_1())
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
                    .child("[x] terminada")
                    .child(marked("[*]", theme::SUCCESS, "equipo en vivo")),
            )
    }
}
