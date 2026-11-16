//! Graph view of the monitor (`MonitorA.dc.html`): the user on top, the
//! director in the centre and the roles around it, joined by lines; a message
//! is a dot that travels along its line with a short label. Below, «ahora»
//! with the last events.

use gpui::{ClickEvent, Context, Div, ElementId, FontWeight, SharedString, div, prelude::*, px};
use titan_agent_core::monitor::{AgentStatus, DIRECTOR, Monitor, YOU};
use titan_agent_core::time::ago;

use crate::theme::{self, color, hairline};

use super::MainView;
use super::monitor::{agent_tag, agent_tint, needs_border, status_mark};

/// Size of the drawing, as in the preview.
const WIDTH: f32 = 960.;
const HEIGHT: f32 = 560.;
/// Edges at rest: rgba(253,252,252,0.14).
const EDGE_REST: u32 = 0xfdfcfc24;

/// Where each node goes: centre and size.
#[derive(Clone, Copy)]
struct Slot {
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    /// Path from the director's border to this node's border.
    path: &'static [(f32, f32)],
    /// Centre of the message label: above the nodes for the side lines,
    /// beside the line for the vertical ones.
    label: (f32, f32),
}

const YOU_SLOT: Slot = Slot {
    cx: 480.,
    cy: 48.,
    w: 184.,
    h: 56.,
    path: &[(480., 212.), (480., 76.)],
    label: (552., 140.),
};
const DIRECTOR_SLOT: Slot = Slot {
    cx: 480.,
    cy: 260.,
    w: 220.,
    h: 96.,
    path: &[],
    label: (0., 0.),
};
/// Role slots: left, right, below, below left, below right.
const ROLE_SLOTS: [Slot; 5] = [
    Slot {
        cx: 160.,
        cy: 260.,
        w: 220.,
        h: 96.,
        path: &[(370., 260.), (270., 260.)],
        label: (320., 192.),
    },
    Slot {
        cx: 800.,
        cy: 260.,
        w: 220.,
        h: 96.,
        path: &[(590., 260.), (690., 260.)],
        label: (640., 192.),
    },
    Slot {
        cx: 480.,
        cy: 460.,
        w: 220.,
        h: 96.,
        path: &[(480., 308.), (480., 412.)],
        label: (552., 360.),
    },
    Slot {
        cx: 160.,
        cy: 460.,
        w: 220.,
        h: 96.,
        path: &[(480., 308.), (480., 372.), (160., 372.), (160., 412.)],
        label: (320., 392.),
    },
    Slot {
        cx: 800.,
        cy: 460.,
        w: 220.,
        h: 96.,
        path: &[(480., 308.), (480., 372.), (800., 372.), (800., 412.)],
        label: (640., 392.),
    },
];

/// Slot of each role: the first step on the left, the reviewer on the
/// right, the rest below. Roles beyond the five slots are not drawn (the
/// other views list them).
fn role_slots(monitor: &Monitor, review_role: Option<&str>) -> Vec<(usize, Slot)> {
    let roles: Vec<usize> = (DIRECTOR + 1..monitor.agents.len()).collect();
    let reviewer = roles
        .iter()
        .copied()
        .find(|&i| Some(monitor.agents[i].id.as_str()) == review_role);
    let mut order: Vec<usize> = Vec::new();
    let mut rest = roles.iter().copied().filter(|&i| Some(i) != reviewer);
    if let Some(first) = rest.next() {
        order.push(first);
    }
    match reviewer {
        Some(r) => order.push(r),
        None => order.extend(rest.next()),
    }
    order.extend(rest);
    order.into_iter().zip(ROLE_SLOTS).collect()
}

/// A point at `t` (0 to 1) along a path.
fn along(path: &[(f32, f32)], t: f32) -> (f32, f32) {
    let lengths: Vec<f32> = path
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).abs() + (w[1].1 - w[0].1).abs())
        .collect();
    let total: f32 = lengths.iter().sum();
    let mut left = total * t.clamp(0., 1.);
    for (w, length) in path.windows(2).zip(&lengths) {
        if left <= *length && *length > 0. {
            let f = left / length;
            return (
                w[0].0 + (w[1].0 - w[0].0) * f,
                w[0].1 + (w[1].1 - w[0].1) * f,
            );
        }
        left -= length;
    }
    *path.last().unwrap_or(&(0., 0.))
}

impl MainView {
    pub(super) fn graph_view(&self, monitor: &Monitor, cx: &mut Context<Self>) -> Div {
        let review_role = self
            .teams
            .iter()
            .find(|r| r.task == monitor.task)
            .and_then(|r| r.review_role.clone());
        let mut slots: Vec<(usize, Slot)> = vec![(YOU, YOU_SLOT), (DIRECTOR, DIRECTOR_SLOT)];
        slots.extend(role_slots(monitor, review_role.as_deref()));
        let current = monitor.current_message();
        let mut canvas = div().relative().w(px(WIDTH)).h(px(HEIGHT)).flex_none();

        // Edges.
        for (index, slot) in slots.iter().filter(|(i, _)| *i != DIRECTOR) {
            let lit = current.filter(|m| {
                (m.from == *index && m.to == DIRECTOR) || (m.to == *index && m.from == DIRECTOR)
            });
            let edge_color = match lit {
                Some(m) => color(agent_tint(&monitor.agents[m.from])),
                None => gpui::rgba(EDGE_REST),
            };
            for w in slot.path.windows(2) {
                let (x0, x1) = (w[0].0.min(w[1].0), w[0].0.max(w[1].0));
                let (y0, y1) = (w[0].1.min(w[1].1), w[0].1.max(w[1].1));
                canvas = canvas.child(
                    div()
                        .absolute()
                        .left(px(x0 - 1.))
                        .top(px(y0 - 1.))
                        .w(px((x1 - x0).max(0.) + 2.))
                        .h(px((y1 - y0).max(0.) + 2.))
                        .bg(edge_color),
                );
            }
        }

        // The message in flight: a dot from the sender along the line, and
        // its label.
        let mut label = None;
        if let Some(m) = current {
            let other = if m.from == DIRECTOR { m.to } else { m.from };
            if let Some((_, slot)) = slots.iter().find(|(i, _)| *i == other) {
                let progress = monitor.dot_progress(m);
                let t = if m.from == DIRECTOR {
                    progress
                } else {
                    1. - progress
                };
                let (x, y) = along(slot.path, t);
                let tint = agent_tint(&monitor.agents[m.from]);
                canvas = canvas.child(
                    div()
                        .absolute()
                        .left(px(x - 5.))
                        .top(px(y - 5.))
                        .size(px(10.))
                        .rounded_full()
                        .bg(color(tint)),
                );
                label = Some((slot.label, tint, m.label.clone()));
            }
        }

        // Nodes.
        for (index, slot) in &slots {
            canvas = canvas.child(self.graph_node(monitor, *index, slot, cx));
        }
        // The label goes over the nodes, centred on its anchor.
        if let Some(((cx_, cy_), tint, text)) = label {
            canvas = canvas.child(
                div()
                    .absolute()
                    .left(px(cx_ - 130.))
                    .top(px(cy_ - 10.))
                    .w(px(260.))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .max_w(px(260.))
                            .px(px(6.))
                            .py(px(2.))
                            .bg(color(theme::SURFACE))
                            .border_1()
                            .border_color(color(tint))
                            .rounded(px(4.))
                            .text_size(px(11.))
                            .text_color(color(theme::INK))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(text),
                    ),
            );
        }

        let feed = monitor.feed.iter().take(4).enumerate().map(|(i, item)| {
            let when = ago(item.at_ms, monitor.now_ms);
            div()
                .flex()
                .gap(px(8.))
                .opacity(1. - i as f32 * 0.12)
                .child(
                    div()
                        .w(px(88.))
                        .flex_none()
                        .text_color(color(theme::STONE))
                        .child(when),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(color(theme::BODY))
                        .child(item.text.clone()),
                )
        });
        div()
            .flex()
            .flex_col()
            .flex_grow_1()
            .child(
                div()
                    .flex()
                    .justify_center()
                    .pt(px(24.))
                    .px(px(16.))
                    .pb(px(8.))
                    .child(canvas),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(24.))
                    .px(px(16.))
                    .pt(px(12.))
                    .pb(px(16.))
                    .border_t_1()
                    .border_color(hairline())
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .flex_1()
                            .min_w(px(420.))
                            .child(div().text_color(color(theme::MUTE)).child("AHORA"))
                            .children(feed),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .justify_end()
                            .gap(px(6.))
                            .w(px(300.))
                            .text_size(px(11.))
                            .text_color(color(theme::MUTE))
                            .child("el punto viaja con el color de quien envía")
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .child(div().text_color(color(theme::CLAUDE)).child("*"))
                                    .child("Claude Code ·")
                                    .child(div().text_color(color(theme::CODEX)).child("*"))
                                    .child("Codex"),
                            )
                            .child("clic en un nodo: abre su sesión"),
                    ),
            )
    }

    fn graph_node(
        &self,
        monitor: &Monitor,
        index: usize,
        slot: &Slot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let agent = &monitor.agents[index];
        let tint = agent_tint(agent);
        let current = monitor.current_message();
        let sending = current.is_some_and(|m| m.from == index);
        let receiving = current.is_some_and(|m| m.to == index);
        let (mut mark, mut mark_color) = status_mark(agent.status, monitor.now_ms);
        let mut border = hairline();
        let mut active = false;
        if agent.status == AgentStatus::Working {
            border = color(theme::SUCCESS);
            active = true;
        }
        if sending && index != YOU {
            mark = "[>]".into();
            mark_color = tint;
            border = color(tint);
            active = true;
        }
        if receiving {
            border = color(agent_tint(&monitor.agents[current.map_or(0, |m| m.from)]));
            active = true;
        }
        if agent.status == AgentStatus::NeedsYou {
            mark = "[?]".into();
            mark_color = theme::WARNING;
            border = needs_border(monitor.now_ms);
            active = true;
        }
        let action = if index == YOU || agent.status == AgentStatus::NeedsYou {
            agent.action.clone()
        } else {
            monitor.action_now(index, true)
        };
        let action_color = match agent.status {
            AgentStatus::NeedsYou => 0xffb340,
            _ if active => theme::BODY,
            _ => theme::MUTE,
        };
        // The user's node is short: its action, when it has one, takes the
        // place of «usuario».
        let (sub, action) = if index == YOU && !action.is_empty() {
            (action, String::new())
        } else {
            (String::new(), action)
        };
        let sub_color = if sub.is_empty() {
            theme::MUTE
        } else {
            action_color
        };
        let sub = match index {
            _ if !sub.is_empty() => sub,
            YOU => "usuario".to_string(),
            DIRECTOR => format!("{} · claude code", agent.label),
            _ => format!(
                "{} · {}",
                agent.label,
                agent.model.clone().unwrap_or_default()
            ),
        };
        let session = agent.session.clone();
        div()
            .id(ElementId::Name(SharedString::from(format!(
                "node-{}",
                agent.id
            ))))
            .absolute()
            .left(px(slot.cx - slot.w / 2.))
            .top(px(slot.cy - slot.h / 2.))
            .w(px(slot.w))
            .h(px(slot.h))
            .flex()
            .flex_col()
            .gap(px(4.))
            .px(px(12.))
            .py(px(8.))
            .bg(color(theme::CANVAS))
            .border_1()
            .border_color(border)
            .rounded(px(4.))
            .opacity(if active { 1. } else { 0.75 })
            .when(session.is_some(), |d| d.cursor_pointer())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.open_agent(session.clone(), cx);
            }))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .child(div().text_color(color(tint)).child(agent_tag(agent)))
                    .child(
                        div()
                            .flex_grow_1()
                            .text_color(color(theme::INK))
                            .font_weight(FontWeight::BOLD)
                            .child(agent.title.clone()),
                    )
                    .child(div().text_color(color(mark_color)).child(mark)),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(color(sub_color))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(sub),
            )
            .when(!action.is_empty(), |d| {
                d.child(
                    div()
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(color(action_color))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(action),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_along_a_path() {
        let path = [(0., 0.), (0., 10.), (10., 10.)];
        assert_eq!(along(&path, 0.), (0., 0.));
        assert_eq!(along(&path, 0.25), (0., 5.));
        assert_eq!(along(&path, 0.75), (5., 10.));
        assert_eq!(along(&path, 1.), (10., 10.));
    }
}
