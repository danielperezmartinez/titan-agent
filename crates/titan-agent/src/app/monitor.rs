//! Team monitor (docs/Decisiones visuales/Monitor del proyecto en tres
//! vistas.md): bar with the task, round and the three view buttons, and the
//! pieces the views share. It opens from a director session's `[m]`
//! (`director_monitor`, docs/Decisiones visuales/Ventana principal con
//! proyectos importados y monitor por sesión directora.md) and, until 0.7.1,
//! from a project's header (`monitor_view`). Each view lives in its own submodule: `graph`,
//! `lanes` and `desk`. What they draw comes from
//! `titan_agent_core::monitor::Board`.

use std::rc::Rc;

use gpui::{
    AnyElement, ClickEvent, Context, Div, FontWeight, Hsla, SharedString, div, prelude::*, px,
};
use titan_agent::conversation::{SessionBody, crumb_name, pick_task, task_hint, toggle_monitor};
use titan_agent_core::monitor::{Agent, Board, TaskId, board, latest_team, run_of};
use titan_agent_core::session::{AgentCli, Session, SessionState};
use titan_agent_core::team::USER;

use crate::theme::{self, color, hairline, hsla};

use super::menu::{MenuItem, menu_list};
use super::thread_view::ThreadRef;
use super::widgets::placeholder;
use super::{MainView, Selection};

mod desk;
mod graph;
mod lanes;

/// What a click on an agent opens: a Codex role's thread read-only (listed
/// by the daemon or not), or the director's session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Target {
    Thread(ThreadRef),
    Session(String),
}

/// The target of an agent; `None` when a click does nothing (and the
/// pointer must not suggest it does).
pub(super) fn target(board: &Board, agent: &Agent) -> Option<Target> {
    if let Some(thread) = &agent.thread {
        return Some(Target::Thread(ThreadRef {
            key: format!("cx:{thread}"),
            title: agent.title.clone(),
            role: agent.role.clone(),
            model: agent.model.clone(),
            task: board.task.clone(),
            root: None,
        }));
    }
    agent.session.clone().map(Target::Session)
}

/// View shown in the monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MonitorTab {
    Graph,
    Lanes,
    Desk,
}

impl MonitorTab {
    const ALL: [(MonitorTab, &'static str); 3] = [
        (MonitorTab::Graph, "grafo"),
        (MonitorTab::Lanes, "carriles"),
        (MonitorTab::Desk, "mesa de control"),
    ];

    /// `TITAN_AGENT_MONITOR` (screenshots): `grafo`, `carriles` or `mesa`.
    pub(super) fn from_env() -> MonitorTab {
        match std::env::var("TITAN_AGENT_MONITOR").as_deref() {
            Ok("carriles") => MonitorTab::Lanes,
            Ok("mesa") => MonitorTab::Desk,
            _ => MonitorTab::Graph,
        }
    }
}

/// Time windows of the lanes view, in seconds.
pub(super) const LANE_WINDOWS: [i64; 3] = [36, 300, 1800];

/// Spinner of a working agent: `| / - \`, one step every 300 ms.
pub(super) fn spinner(now_ms: i64) -> &'static str {
    ["|", "/", "-", "\\"][(now_ms / 300).rem_euclid(4) as usize]
}

/// 0 to 1 and back over `period_ms`, for things that pulse.
pub(super) fn pulse(now_ms: i64, period_ms: i64) -> f32 {
    let phase = (now_ms.rem_euclid(period_ms)) as f32 / period_ms as f32;
    0.5 - 0.5 * (phase * std::f32::consts::TAU).cos()
}

pub(super) fn agent_hex(cli: Option<AgentCli>) -> u32 {
    cli.map_or(theme::INK, theme::agent_color)
}

pub(super) fn with_alpha(hex: u32, alpha: f32) -> Hsla {
    let mut c = hsla(hex);
    c.a = alpha;
    c
}

/// How an agent looks right now: marker, its colour, action colour, border.
pub(super) struct Look {
    pub marker: String,
    pub marker_hex: u32,
    pub action: String,
    pub action_hex: u32,
    pub border: Hsla,
    pub dim: bool,
}

/// Look of an agent, given the message travelling now.
pub(super) fn look(board: &Board, agent: &Agent, now_ms: i64) -> Look {
    let live = board.live_message(now_ms);
    let mut l = Look {
        marker: "[+]".into(),
        marker_hex: theme::MUTE,
        action: agent.action.clone(),
        action_hex: theme::MUTE,
        border: hairline().into(),
        dim: true,
    };
    match agent.state {
        SessionState::Working => {
            l.marker = format!("[>] {}", spinner(now_ms));
            l.marker_hex = theme::SUCCESS;
            l.action_hex = theme::BODY;
            l.border = hsla(theme::SUCCESS);
            l.dim = false;
        }
        SessionState::Finished { .. } => l.marker = "[x]".into(),
        _ => {}
    }
    if let Some(m) = live {
        if m.from == agent.id && m.to != USER {
            l.marker = "[>]".into();
            l.marker_hex = agent_hex(agent.cli);
            l.action = format!("envía a {}: {}", board.label(&m.to), m.text);
            l.action_hex = theme::BODY;
            l.border = hsla(agent_hex(agent.cli));
            l.dim = false;
        }
        if m.to == agent.id {
            l.action = if agent.cli.is_none() {
                "te piden una decisión".into()
            } else {
                format!("recibe de {}: {}", board.label(&m.from), m.text)
            };
            l.action_hex = theme::BODY;
            l.border = hsla(agent_hex(m.sender));
            l.dim = false;
        }
    }
    if agent.state == SessionState::NeedsYou {
        l.marker = "[?]".into();
        l.marker_hex = theme::WARNING;
        if agent.cli.is_some() {
            l.action = "te necesita".into();
        }
        l.action_hex = 0xffb340;
        l.border = with_alpha(theme::WARNING, 0.45 + 0.55 * pulse(now_ms, 1400));
        l.dim = false;
    }
    // Why the director may be waiting on a role: the app holds its thread,
    // or its last turn was refused because the thread was open elsewhere.
    // Own markers, so they do not read as «[?] te necesita».
    if agent.writer_open {
        l.marker = "[w]".into();
        l.marker_hex = theme::WARNING;
        l.action = if agent.busy {
            "abierto en la app · hilo ocupado".into()
        } else {
            "abierto en la app".into()
        };
        l.action_hex = theme::WARNING;
        l.border = hsla(theme::WARNING);
        l.dim = false;
    } else if agent.busy && agent.state != SessionState::Working {
        l.marker = "[!]".into();
        l.marker_hex = theme::WARNING;
        l.action = "hilo ocupado".into();
        l.action_hex = theme::BODY;
        l.dim = false;
    }
    l
}

/// «N s en este estado» / «N min en este estado».
pub(super) fn in_state(since_ms: Option<i64>, now_ms: i64) -> String {
    let Some(since) = since_ms else {
        return String::new();
    };
    let seconds = ((now_ms - since) / 1000).max(0);
    if seconds < 60 {
        format!("{seconds} s en este estado")
    } else if seconds < 3600 {
        format!("{} min en este estado", seconds / 60)
    } else {
        format!("{} h en este estado", seconds / 3600)
    }
}

impl MainView {
    /// The board of the project's newest open task, if it has one.
    pub(super) fn board_for(&self, root: &str) -> Option<Board> {
        let (task, run) = latest_team(&self.teams, root)?;
        let mut board = board(&task, &run, &self.sessions, self.now_ms);
        board.mark_writers(&self.daemon_terminals);
        Some(board)
    }

    /// The board of one task, while it is open.
    fn board_of(&self, id: &TaskId) -> Option<Board> {
        let task = self.teams.iter().find(|t| id.matches(t))?;
        let run = run_of(task).filter(|run| !run.closed)?;
        let mut board = board(task, &run, &self.sessions, self.now_ms);
        board.mark_writers(&self.daemon_terminals);
        Some(board)
    }

    /// Open tasks a session directs, the newest first (empty when it
    /// directs none).
    pub(super) fn director_tasks(&self, key: &str) -> &[TaskId] {
        self.directors.get(key).map_or(&[], Vec::as_slice)
    }

    /// `[m]` in a director's bar: its monitor, or back to its conversation.
    pub(super) fn toggle_monitor(&mut self, key: &str, cx: &mut Context<Self>) {
        self.body = toggle_monitor(&self.body, self.director_tasks(key));
        self.task_menu = false;
        cx.notify();
    }

    /// Whether the centre shows a live monitor (it then redraws often).
    pub(super) fn monitor_animating(&self) -> bool {
        match &self.selection {
            Selection::Project(root) => self.teams.iter().any(|t| &t.project_root == root),
            Selection::Session(_) => matches!(self.body, SessionBody::Monitor { .. }),
            _ => false,
        }
    }

    /// Opens what a click on an agent of the monitor targets.
    pub(super) fn open_agent(
        &mut self,
        target: Target,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            Target::Thread(mut r) => {
                r.root = match &self.selection {
                    Selection::Project(root) => Some(root.clone()),
                    Selection::Session(key) => self.session(key).map(|s| s.project_root.clone()),
                    _ => None,
                };
                self.open_thread(r, cx)
            }
            Target::Session(key) => self.select_session(key, Some(window), cx),
        }
    }

    /// The three view buttons of the monitor's bar.
    fn monitor_tabs(&self, cx: &mut Context<Self>) -> Div {
        let tabs = MonitorTab::ALL.map(|(tab, label)| {
            let selected = self.monitor_tab == tab;
            div()
                .id(SharedString::from(format!("monitor-tab-{label}")))
                .flex()
                .items_center()
                .h(px(30.))
                .px(px(12.))
                .border_1()
                .rounded(px(4.))
                .border_color(if selected {
                    color(theme::ACCENT).into()
                } else {
                    Hsla::from(hairline())
                })
                .when(selected, |d| d.bg(color(theme::SURFACE_ELEVATED)))
                .text_color(color(if selected { theme::INK } else { theme::MUTE }))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.monitor_tab = tab;
                    cx.notify();
                }))
                .child(label)
        });
        div().flex().flex_none().gap(px(4.)).children(tabs)
    }

    /// `[*] en vivo`, pulsing.
    fn live_marker(&self) -> Div {
        div()
            .flex()
            .flex_none()
            .gap(px(6.))
            .text_color(color(theme::SUCCESS))
            .child(
                div()
                    .opacity(0.25 + 0.75 * pulse(self.now_ms + 800, 1600))
                    .child("[*]"),
            )
            .child("en vivo")
    }

    /// The bar of a monitor (40 px, hairline below).
    fn monitor_bar() -> Div {
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
    }

    /// One of the three views of a board.
    fn board_view(&self, board: &Board, cx: &mut Context<Self>) -> AnyElement {
        match self.monitor_tab {
            MonitorTab::Graph => self.graph_view(board, cx).into_any_element(),
            MonitorTab::Lanes => self.lanes_view(board, cx).into_any_element(),
            MonitorTab::Desk => self.desk_view(board, cx).into_any_element(),
        }
    }

    /// Bar, body and the notice line below.
    fn monitor_frame(&self, bar: Div, menu: Option<Div>, body: AnyElement) -> Div {
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(bar)
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_grow_1()
                    .min_h(px(0.))
                    .bg(color(theme::TERMINAL_BG))
                    .child(body)
                    // A menu of the bar floats over the body, under it.
                    .children(menu.map(|menu| {
                        div()
                            .absolute()
                            .top(px(4.))
                            .left(px(16.))
                            .w(px(440.))
                            .child(menu)
                    })),
            )
            .when_some(self.notice.clone(), |d, (text, error)| {
                d.child(
                    div()
                        .px(px(16.))
                        .py(px(6.))
                        .bg(color(theme::TERMINAL_BG))
                        .text_size(px(12.))
                        .text_color(color(if error { theme::DANGER } else { theme::MUTE }))
                        .child(text),
                )
            })
    }

    /// The monitor opened from a project's header: its newest open task
    /// (until 0.7.1 replaces it with the project's summary).
    pub(super) fn monitor_view(&self, root: &str, cx: &mut Context<Self>) -> Div {
        let sessions: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|s| s.project_root == root)
            .collect();
        let name = sessions
            .first()
            .map(|s| s.project.clone())
            .unwrap_or_else(|| titan_agent_core::project::project_name(root));
        let board = self.board_for(root);
        let subtitle = match &board {
            Some(b) => {
                let mut text = format!("encargo {}", b.task);
                if b.max_rounds > 0 {
                    text.push_str(&format!(" · ronda {} de {}", b.round.max(1), b.max_rounds));
                }
                text.push_str(&format!(" · {} agentes", b.agents.len() - 1));
                text
            }
            None => format!(
                "{} sesión(es), ninguna en equipo",
                sessions.iter().filter(|s| s.parent.is_none()).count()
            ),
        };
        let bar = Self::monitor_bar()
            .child(
                div()
                    .text_color(color(theme::INK))
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(13.))
                    .child(format!("[#] {name}")),
            )
            .child(
                div()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(subtitle),
            )
            .when(board.is_some(), |d| d.child(self.live_marker()))
            .child(div().flex_grow_1())
            .child(self.monitor_tabs(cx));
        let body = match board {
            Some(board) => self.board_view(&board, cx),
            None => {
                let loose: Vec<String> = sessions
                    .iter()
                    .filter(|s| s.parent.is_none())
                    .map(|s| s.name.clone())
                    .collect();
                let mut list = loose.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
                if loose.len() > 6 {
                    list.push_str(&format!(" y {} más", loose.len() - 6));
                }
                placeholder(
                    &format!("Sin equipo trabajando en {name}"),
                    vec![
                        "El monitor se anima cuando las sesiones del proyecto se encargan trabajo entre ellas."
                            .into(),
                        format!(
                            "Sesiones sueltas: {}",
                            if list.is_empty() { "ninguna".into() } else { list }
                        ),
                    ],
                )
                .into_any_element()
            }
        };
        self.monitor_frame(bar, None, body)
    }

    /// The monitor of one of a director's tasks, in place of its terminal
    /// (`[m]`): crumb `[#] proyecto › directora`, the task picker when it
    /// directs more than one, and `[<] conversación` to go back.
    pub(super) fn director_monitor(
        &self,
        session: &Session,
        task: &TaskId,
        cx: &mut Context<Self>,
    ) -> Div {
        let tasks = self.director_tasks(&session.key).to_vec();
        let board = self.board_of(task);
        let many = tasks.len() > 1;
        let picker = div()
            .id("task-picker")
            .flex()
            .flex_shrink(1.)
            .min_w(px(120.))
            .overflow_hidden()
            .gap(px(6.))
            .px(px(8.))
            .h(px(28.))
            .items_center()
            // A box only when it opens the picker (more than one task).
            .when(many, |d| {
                d.border_1()
                    .rounded(px(4.))
                    .border_color(if self.task_menu {
                        Hsla::from(color(theme::ACCENT))
                    } else {
                        Hsla::from(hairline())
                    })
            })
            .child(div().whitespace_nowrap().child("encargo"))
            .child(
                div()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(color(theme::BODY))
                    .child(task.task.clone()),
            )
            .when(many, |d| {
                d.cursor_pointer()
                    .child(div().text_color(color(theme::MUTE)).child("[v]"))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.task_menu = !this.task_menu;
                        cx.notify();
                    }))
            });
        let round = board
            .as_ref()
            .filter(|b| b.max_rounds > 0)
            .map(|b| format!("ronda {} de {}", b.round.max(1), b.max_rounds));
        let bar = Self::monitor_bar()
            .child(
                div()
                    .flex()
                    // The crumb keeps its width (it tells two directors
                    // apart); the round gives way first, then the picker.
                    .flex_none()
                    .overflow_hidden()
                    .gap(px(6.))
                    .text_size(px(13.))
                    .child(
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .child(format!("[#] {} ›", session.project)),
                    )
                    .child(
                        div()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(color(theme::INK))
                            .font_weight(FontWeight::BOLD)
                            .whitespace_nowrap()
                            .child(crumb_name(&session.name, &session.project).to_string()),
                    ),
            )
            .child(picker)
            .when_some(round, |d, round| {
                d.child(
                    div()
                        .whitespace_nowrap()
                        .flex_shrink(8.)
                        .min_w(px(0.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(round),
                )
            })
            .when(board.is_some(), |d| d.child(self.live_marker()))
            .child(div().flex_grow_1())
            .child(self.monitor_tabs(cx))
            .child(
                div()
                    .id("back-to-conversation")
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(30.))
                    .px(px(12.))
                    .border_1()
                    .rounded(px(4.))
                    .border_color(hairline())
                    .text_color(color(theme::BODY))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.body = SessionBody::Terminal;
                        this.task_menu = false;
                        cx.notify();
                    }))
                    .child("[<] conversación"),
            );
        let menu = (self.task_menu && many).then(|| {
            let items = tasks
                .iter()
                .map(|id| {
                    let found = self.teams.iter().find(|t| id.matches(t)).and_then(run_of);
                    let hint = found.map_or_else(String::new, |run| task_hint(&run, self.now_ms));
                    let pick = id.clone();
                    MenuItem {
                        tag: "[*]".into(),
                        tag_color: theme::SUCCESS,
                        label: id.task.clone(),
                        hint,
                        selected: id == task,
                        pick: Rc::new(move |this: &mut MainView, cx: &mut Context<MainView>| {
                            let key = match &this.selection {
                                Selection::Session(key) => key.clone(),
                                _ => return,
                            };
                            this.body = pick_task(&this.body, &pick, this.director_tasks(&key));
                            this.task_menu = false;
                            cx.notify();
                        }),
                    }
                })
                .collect();
            menu_list("encargos abiertos de esta sesión", items, cx)
        });
        let body = match board {
            Some(board) => self.board_view(&board, cx),
            None => placeholder(
                "El encargo ya no está abierto",
                vec!["Vuelve a la conversación con [<] o elige otro encargo.".into()],
            )
            .into_any_element(),
        };
        self.monitor_frame(bar, menu, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_and_pulse_cycle() {
        assert_eq!(
            [0, 300, 600, 900, 1200].map(spinner),
            ["|", "/", "-", "\\", "|"]
        );
        assert!(pulse(0, 1000) < 0.01);
        assert!((pulse(500, 1000) - 1.0).abs() < 0.01);
        assert_eq!(in_state(Some(0), 61_000), "1 min en este estado");
        assert_eq!(in_state(None, 5), "");
    }
}
