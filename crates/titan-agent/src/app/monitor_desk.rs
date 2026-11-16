//! Control desk view of the monitor (`MonitorC.dc.html`, in the style of
//! `htop`): a strip with who talks, works or needs you and the review round,
//! and one card per agent (the director full width) with its state, action,
//! time in that state, recent activity, turns and tokens, and quota.

use gpui::{ClickEvent, Context, Div, ElementId, FontWeight, SharedString, div, prelude::*, px};
use titan_agent_core::monitor::{AgentStatus, DIRECTOR, Monitor, RoundState, Spark, Talk};

use crate::theme::{self, color, hairline};

use super::MainView;
use super::monitor::{
    agent_line, agent_tag, agent_tint, duration, needs_border, spinner, status_mark, tokens,
};

/// Bars of the activity histogram and the time they cover.
const SPARK_BARS: usize = 24;
const SPARK_SPAN_MS: i64 = 6 * 60 * 1000;
/// Length of the ASCII line and the time the `*` takes to cross it.
const LINE_LEN: usize = 24;
const LINE_MS: i64 = 2_400;

impl MainView {
    pub(super) fn desk_view(&self, monitor: &Monitor, cx: &mut Context<Self>) -> Div {
        let roles: Vec<usize> = (DIRECTOR + 1..monitor.agents.len()).collect();
        let mut rows = vec![div().flex().child(self.card(monitor, DIRECTOR, cx))];
        for pair in roles.chunks(2) {
            let mut row = div().flex().gap(px(16.));
            for &i in pair {
                row = row.child(self.card(monitor, i, cx));
            }
            rows.push(row);
        }
        div()
            .flex()
            .flex_col()
            .flex_grow_1()
            .gap(px(16.))
            .p(px(16.))
            .child(self.talk_strip(monitor))
            .children(rows)
    }

    fn talk_strip(&self, monitor: &Monitor) -> Div {
        let now = monitor.now_ms;
        let (title, from, line, to, label, border, from_color, line_color, to_color) =
            match monitor.talk() {
                Talk::Talking {
                    from,
                    to,
                    label,
                    at_ms,
                    needs_you,
                } => {
                    let pos = (((now - at_ms).rem_euclid(LINE_MS)) as usize * LINE_LEN
                        / LINE_MS as usize)
                        .min(LINE_LEN - 1);
                    let mut ascii: String = (0..LINE_LEN)
                        .map(|i| if i == pos { '*' } else { '-' })
                        .collect();
                    ascii.push('>');
                    let sender = agent_tint(&monitor.agents[from]);
                    (
                        if needs_you {
                            "TE NECESITAN"
                        } else {
                            "HABLANDO"
                        },
                        agent_line(monitor, from),
                        ascii,
                        agent_line(monitor, to),
                        label,
                        if needs_you {
                            needs_border(now)
                        } else {
                            color(sender)
                        },
                        sender,
                        sender,
                        agent_tint(&monitor.agents[to]),
                    )
                }
                Talk::Working { agent, label } => (
                    "TRABAJANDO",
                    agent_line(monitor, agent),
                    spinner(now).to_string(),
                    String::new(),
                    label,
                    hairline(),
                    agent_tint(&monitor.agents[agent]),
                    theme::SUCCESS,
                    theme::MUTE,
                ),
                Talk::Idle => (
                    "EN ESPERA",
                    String::new(),
                    String::new(),
                    String::new(),
                    monitor
                        .feed
                        .first()
                        .map(|f| format!("último: {}", f.text))
                        .unwrap_or_default(),
                    hairline(),
                    theme::MUTE,
                    theme::MUTE,
                    theme::MUTE,
                ),
            };
        let rounds = monitor.rounds.iter().map(|state| {
            div().w(px(18.)).h(px(8.)).bg(color(match state {
                RoundState::Ok | RoundState::InProgress => theme::SUCCESS,
                RoundState::Changes => theme::WARNING,
                RoundState::Pending => theme::SURFACE_ELEVATED,
            }))
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.))
            .p(px(16.))
            .bg(color(theme::CANVAS))
            .border_1()
            .border_color(border)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(color(theme::MUTE))
                    .child(title),
            )
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .text_size(px(18.))
                    .child(
                        div()
                            .text_color(color(from_color))
                            .font_weight(FontWeight::BOLD)
                            .child(from),
                    )
                    .child(div().text_color(color(line_color)).child(line))
                    .child(
                        div()
                            .text_color(color(to_color))
                            .font_weight(FontWeight::BOLD)
                            .child(to),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(14.))
                    .text_color(color(theme::BODY))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(label),
            )
            .when(monitor.max_rounds > 0, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(12.))
                        .text_color(color(theme::MUTE))
                        .child("ronda")
                        .child(div().flex().gap(px(3.)).children(rounds))
                        .child(format!(
                            "{} de {}",
                            monitor.round.max(1).min(monitor.rounds.len() as u32),
                            monitor.rounds.len()
                        )),
                )
            })
    }

    fn card(
        &self,
        monitor: &Monitor,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let agent = &monitor.agents[index];
        let now = monitor.now_ms;
        let tint = agent_tint(agent);
        let current = monitor.current_message();
        let (mut mark, mut mark_color) = status_mark(agent.status, now);
        let mut border = hairline();
        if agent.status == AgentStatus::Working {
            border = color(theme::SUCCESS);
        }
        if let Some(m) = current {
            if m.from == index {
                mark = "[>]".into();
                mark_color = tint;
                border = color(tint);
            } else if m.to == index {
                border = color(agent_tint(&monitor.agents[m.from]));
            }
        }
        if agent.status == AgentStatus::NeedsYou {
            mark = "[?]".into();
            mark_color = theme::WARNING;
            border = needs_border(now);
        }
        let action = match agent.status {
            AgentStatus::NeedsYou => format!("te necesita: {}", agent.action)
                .replace("te necesita: te necesita", "te necesita"),
            _ => monitor.action_now(index, false),
        };
        let action_color = match agent.status {
            AgentStatus::NeedsYou => 0xffb340,
            AgentStatus::Waiting | AgentStatus::Done
                if !current.is_some_and(|m| m.from == index || m.to == index) =>
            {
                theme::MUTE
            }
            _ => theme::BODY,
        };
        let sub = if index == DIRECTOR {
            format!(
                "{} · claude code · {}",
                agent.label,
                agent.model.clone().unwrap_or_default()
            )
        } else {
            [
                Some(agent.label.clone()),
                agent.model.clone(),
                agent.effort.clone(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
        };
        let stats = if index == DIRECTOR {
            let since = self
                .teams
                .iter()
                .find(|r| r.task == monitor.task)
                .map_or(now, |r| r.created_ms);
            format!(
                "encargos {} · en este encargo {}",
                agent.turns,
                duration(now - since)
            )
        } else {
            format!(
                "turnos {} · {} tok",
                agent.turns,
                tokens(agent.tokens.unwrap_or(0))
            )
        };
        let spark = monitor
            .spark(index, SPARK_BARS, SPARK_SPAN_MS)
            .into_iter()
            .map(|s| {
                let (h, c) = match s {
                    Spark::Working => (30., theme::SUCCESS),
                    Spark::NeedsYou => (22., theme::WARNING),
                    Spark::Message(from) => (14., agent_tint(&monitor.agents[from])),
                    Spark::Idle => (3., theme::SURFACE_ELEVATED),
                };
                div().flex_1().h(px(h)).bg(color(c))
            });
        let (quota_label, filled, low) = match &agent.quota {
            Some(q) => {
                let who = match &q.model {
                    Some(model) => titan_agent_core::session::short_model(model),
                    None => q.limit.clone(),
                };
                let low = q.is_low();
                (
                    format!(
                        "cuota {who} ({}){}",
                        q.window_label(),
                        if low { " · baja" } else { "" }
                    ),
                    (q.remaining_percent() / 10.).round() as usize,
                    low,
                )
            }
            None => (
                match agent.agent {
                    Some(cli) => format!("cuota {} · desconocida", cli.command()),
                    None => String::new(),
                },
                0,
                false,
            ),
        };
        let blocks = (0..10).map(|i| {
            div().w(px(8.)).h(px(10.)).bg(color(if i < filled {
                if low { theme::WARNING } else { theme::MUTE }
            } else {
                theme::SURFACE_ELEVATED
            }))
        });
        let session = agent.session.clone();
        div()
            .id(ElementId::Name(SharedString::from(format!(
                "card-{}",
                agent.id
            ))))
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(16.))
            .bg(color(theme::CANVAS))
            .border_1()
            .border_color(border)
            .rounded(px(4.))
            .when(session.is_some(), |d| d.cursor_pointer())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.open_agent(session.clone(), cx);
            }))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(color(tint))
                            .child(agent_tag(agent)),
                    )
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(color(theme::INK))
                            .child(agent.title.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(12.))
                            .text_color(color(theme::MUTE))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(sub),
                    )
                    .child(
                        div()
                            .text_size(px(16.))
                            .text_color(color(mark_color))
                            .child(mark),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(14.))
                            .text_color(color(action_color))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(action),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(color(theme::MUTE))
                            .child(format!("{} en este estado", duration(now - agent.since_ms))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(3.))
                    .h(px(32.))
                    .children(spark),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .text_size(px(11.))
                    .text_color(color(theme::MUTE))
                    .child(stats)
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(quota_label)
                            .child(div().flex().gap(px(2.)).children(blocks)),
                    ),
            )
    }
}
