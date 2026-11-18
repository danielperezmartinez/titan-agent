//! Summary of a project, opened from its header in the tree (docs/Decisiones
//! visuales/Ventana principal con proyectos importados y monitor por sesión
//! directora.md): its folder and actions, its teams (open tasks, each with
//! its director and `[m] monitor`) and its live sessions. A summary, not a
//! control panel. What it lists comes from `project_teams` and
//! `project_tree` of the core.

use gpui::{ClickEvent, Context, Div, FontWeight, SharedString, Stateful, div, prelude::*, px};
use titan_agent::conversation::{SessionBody, crumb_name, pick_task};
use titan_agent_core::monitor::{TaskId, project_teams};
use titan_agent_core::project::{project_name, tilde};
use titan_agent_core::protocol::Request;
use titan_agent_core::time::ago;
use titan_agent_core::tree::{NavRow, project_tree};

use crate::theme::{self, color, hairline};

use super::widgets::{agent_pill, state_marker};
use super::{MainView, Pending, ProjectBody};

/// An action of the summary: `[x] text` with a hairline border.
fn action(id: String, label: &'static str, text: u32) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .flex()
        .flex_none()
        .items_center()
        .h(px(30.))
        .px(px(12.))
        .border_1()
        .rounded(px(4.))
        .border_color(hairline())
        .text_size(px(12.))
        .text_color(color(text))
        .whitespace_nowrap()
        .cursor_pointer()
        .child(label)
}

fn section(title: String) -> Div {
    div()
        .pt(px(20.))
        .pb(px(6.))
        .text_size(px(12.))
        .text_color(color(theme::MUTE))
        .child(title)
}

impl MainView {
    /// `[m]` of a team: its director's own monitor, or the monitor here
    /// when no director session is known.
    pub(super) fn open_team(
        &mut self,
        task: TaskId,
        director: Option<String>,
        cx: &mut Context<Self>,
    ) {
        match director {
            Some(key) => {
                self.select_session(key.clone(), None, cx);
                self.body = pick_task(&SessionBody::Terminal, &task, self.director_tasks(&key));
            }
            None => self.project_body = ProjectBody::Monitor(task),
        }
        cx.notify();
    }

    pub(super) fn summary_view(&self, root: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let info = self
            .projects
            .iter()
            .flatten()
            .find(|p| p.root == root)
            .cloned();
        let imported = info.is_some() || self.projects.is_none();
        let missing = info.as_ref().is_some_and(|p| p.missing);
        let name = info.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| {
            self.sessions
                .iter()
                .find(|s| s.project_root == root)
                .map(|s| s.project.clone())
                .unwrap_or_else(|| project_name(root))
        });
        let teams = project_teams(root, &self.teams, &self.sessions, self.now_ms);
        let sessions: Vec<String> = project_tree(
            &self.sessions,
            self.projects.as_deref().unwrap_or_default(),
            &self.teams,
            "",
        )
        .into_iter()
        .skip_while(|row| !matches!(row, NavRow::Project { root: r, .. } if r == root))
        .skip(1)
        .take_while(|row| matches!(row, NavRow::Session { .. }))
        .filter_map(|row| match row {
            NavRow::Session { key, .. } => Some(key),
            _ => None,
        })
        .filter(|key| self.session(key).is_some_and(|s| s.state.is_live()))
        .collect();

        let new_root = root.to_string();
        let open_root = root.to_string();
        let edit_root = root.to_string();
        let actions =
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .pt(px(16.))
                .child(
                    action(format!("sum-new:{root}"), "[+] nueva sesión", theme::INK).on_click(
                        cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.open_new_in(new_root.clone(), window, cx)
                        }),
                    ),
                )
                .when(!missing, |d| {
                    d.child(
                        action(format!("sum-open:{root}"), "[d] abrir carpeta", theme::BODY)
                            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                                cx.open_with_system(std::path::Path::new(&open_root))
                            })),
                    )
                })
                .when(self.projects.is_some(), |d| {
                    d.child(if imported {
                        action(format!("sum-remove:{root}"), "[x] quitar", theme::DANGER).on_click(
                            cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.remove_project(edit_root.clone(), cx)
                            }),
                        )
                    } else {
                        action(format!("sum-import:{root}"), "[+] importar", theme::ACCENT)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.import_project(edit_root.clone(), cx)
                            }))
                    })
                });

        let team_rows = teams.into_iter().map(|line| {
            let director_name = line
                .director
                .as_deref()
                .and_then(|key| self.session(key))
                .map(|s| crumb_name(&s.name, &s.project).to_string());
            let round = if line.max_rounds == 0 {
                "sin revisión".to_string()
            } else {
                format!("ronda {} de {}", line.round.max(1), line.max_rounds)
            };
            let task = line.id.clone();
            let director = line.director.clone();
            let open = director.clone();
            let id = SharedString::from(format!("team:{}:{}", line.id.folder, line.id.task));
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .min_h(px(36.))
                .border_b_1()
                .border_color(hairline())
                .text_size(px(13.))
                .child(
                    div()
                        .flex_none()
                        .text_color(color(theme::state_color(line.state)))
                        .child(state_marker(line.state, self.now_ms)),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(color(theme::INK))
                        .font_weight(FontWeight::MEDIUM)
                        .child(line.id.task.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(color(theme::MUTE))
                        .child(round),
                )
                .child(match director_name {
                    Some(name) => div()
                        .id(SharedString::from(format!("{id}:director")))
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(color(theme::BODY))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            if let Some(key) = open.clone() {
                                this.select_session(key, Some(window), cx);
                            }
                        }))
                        .child(format!("directora {name}")),
                    None => div()
                        .id(SharedString::from(format!("{id}:director")))
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(color(theme::STONE))
                        .child("sin sesión directora"),
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.))
                        .text_color(color(theme::MUTE))
                        .child(format!(
                            "{} · {}",
                            line.headline,
                            ago(line.last_ts_ms, self.now_ms)
                        )),
                )
                .child(
                    action(format!("{id}:monitor"), "[m] monitor", theme::BODY).on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.open_team(task.clone(), director.clone(), cx)
                        }),
                    ),
                )
        });
        let team_count = project_teams(root, &self.teams, &self.sessions, self.now_ms).len();

        let session_rows = sessions.iter().filter_map(|key| {
            let session = self.session(key)?;
            let agent = theme::agent_color(session.agent);
            let since = session
                .state_since_ms
                .map(|at| ago(at, self.now_ms).replace("hace ", "desde hace "))
                .unwrap_or_default();
            let key = key.clone();
            Some(
                div()
                    .id(SharedString::from(format!("sum-s:{key}")))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .min_h(px(34.))
                    .border_b_1()
                    .border_color(hairline())
                    .text_size(px(13.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.select_session(key.clone(), Some(window), cx)
                    }))
                    .child(
                        div()
                            .flex_none()
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
                            .child(session.name.clone()),
                    )
                    .child(agent_pill(session.agent, session.model.as_deref(), agent))
                    .child(div().flex_grow_1())
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(color(theme::MUTE))
                            .child(format!("{} {since}", session.state.label())),
                    ),
            )
        });

        div()
            .id("summary")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .px(px(24.))
            .py(px(20.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_color(color(theme::INK))
                            .text_size(px(18.))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("[#] {name}")),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(color(theme::MUTE))
                            .child(tilde(root, self.home.as_deref())),
                    )
                    .when(missing, |d| {
                        d.child(
                            div()
                                .text_size(px(12.))
                                .text_color(color(theme::WARNING))
                                .child("[!] carpeta no encontrada"),
                        )
                    })
                    .when(!imported, |d| {
                        d.child(
                            div()
                                .text_size(px(12.))
                                .text_color(color(theme::STONE))
                                .child("sin importar"),
                        )
                    }),
            )
            .child(actions)
            .child(section(match team_count {
                0 => "EQUIPOS · ningún encargo abierto".into(),
                1 => "EQUIPOS · 1 encargo abierto".into(),
                n => format!("EQUIPOS · {n} encargos abiertos"),
            }))
            .children(team_rows)
            .child(section(match sessions.len() {
                0 => "SESIONES · ninguna viva".into(),
                1 => "SESIONES · 1 viva".into(),
                n => format!("SESIONES · {n} vivas"),
            }))
            .children(session_rows)
            .when_some(self.notice.clone(), |d, (text, error)| {
                d.child(
                    div()
                        .pt(px(16.))
                        .text_size(px(12.))
                        .text_color(color(if error { theme::DANGER } else { theme::MUTE }))
                        .child(text),
                )
            })
    }

    /// `[+] importar` (a group of «otras carpetas», or a folder picked from
    /// the tree's title).
    pub(super) fn import_project(&mut self, path: String, cx: &mut Context<Self>) {
        let id = self.client.request(Request::ProjectAdd { path });
        self.pending.insert(id, Pending::Projects);
        cx.notify();
    }

    /// `[x] quitar`: only from the list; the folder is never touched.
    pub(super) fn remove_project(&mut self, root: String, cx: &mut Context<Self>) {
        let id = self.client.request(Request::ProjectRemove { root });
        self.pending.insert(id, Pending::Projects);
        cx.notify();
    }
}
