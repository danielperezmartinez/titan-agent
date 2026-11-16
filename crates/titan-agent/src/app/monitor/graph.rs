//! Graph view of the monitor (preview `MonitorA`): the user on top, the
//! director in the centre and the roles around it, joined by lines; a
//! message is a dot travelling along its line with a short label. Below,
//! «ahora» with the last events.

use gpui::{AnyElement, ClickEvent, Context, Div, FontWeight, SharedString, div, prelude::*, px};
use titan_agent_core::monitor::{Agent, Board};
use titan_agent_core::team::USER;
use titan_agent_core::time::ago;

use crate::theme::{self, color, hairline, hsla};

use super::super::MainView;
use super::{agent_hex, look};

const WIDTH: f32 = 960.;
const HEIGHT: f32 = 560.;
/// One trip of a message dot along its line.
const TRIP_MS: i64 = 1100;
/// Width of the label of a message on a horizontal line.
const LABEL_W: f32 = 180.;

/// Centre and size of a node.
#[derive(Clone, Copy)]
struct Place {
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
}

const USER_PLACE: Place = Place {
    cx: 480.,
    cy: 48.,
    w: 160.,
    h: 56.,
};
const DIRECTOR_PLACE: Place = Place {
    cx: 480.,
    cy: 260.,
    w: 220.,
    h: 96.,
};

/// A slot: the node, its line from the director and where labels go.
type Slot = (Place, Vec<(f32, f32)>, (f32, f32));

/// Slots of the other roles: left, right, bottom, bottom-left, bottom-right.
/// Each comes with its line from the director, as points from the
/// director's side, and where the label of a message goes.
fn slots() -> [Slot; 5] {
    let place = |cx, cy| Place {
        cx,
        cy,
        w: 220.,
        h: 96.,
    };
    [
        (
            place(160., 260.),
            vec![(370., 260.), (270., 260.)],
            (262., 222.),
        ),
        (
            place(800., 260.),
            vec![(590., 260.), (690., 260.)],
            (582., 222.),
        ),
        (
            place(480., 460.),
            vec![(480., 308.), (480., 412.)],
            (492., 350.),
        ),
        (
            place(160., 460.),
            vec![(420., 308.), (420., 360.), (160., 360.), (160., 412.)],
            (172., 330.),
        ),
        (
            place(800., 460.),
            vec![(540., 308.), (540., 360.), (800., 360.), (800., 412.)],
            (640., 330.),
        ),
    ]
}

/// Point at `fraction` (0 to 1) of a polyline.
fn along(points: &[(f32, f32)], fraction: f32) -> (f32, f32) {
    let lengths: Vec<f32> = points
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .collect();
    let mut left = lengths.iter().sum::<f32>() * fraction.clamp(0., 1.);
    for (segment, length) in points.windows(2).zip(&lengths) {
        if left <= *length && *length > 0. {
            let t = left / length;
            return (
                segment[0].0 + (segment[1].0 - segment[0].0) * t,
                segment[0].1 + (segment[1].1 - segment[0].1) * t,
            );
        }
        left -= length;
    }
    *points.last().unwrap_or(&(0., 0.))
}

/// `12 s`, `3 min`, `2 h`: age of an older «ahora» line.
fn short_age(ms: i64) -> String {
    let seconds = (ms / 1000).max(0);
    match seconds {
        0..=59 => format!("{seconds} s"),
        60..=3599 => format!("{} min", seconds / 60),
        _ => format!("{} h", seconds / 3600),
    }
}

/// A line between two node ids.
struct Edge {
    a: String,
    b: String,
    points: Vec<(f32, f32)>,
    label: (f32, f32),
}

impl MainView {
    pub(super) fn graph_view(&self, board: &Board, cx: &mut Context<Self>) -> Div {
        let now = self.now_ms;
        let mut places: Vec<(&Agent, Place)> = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();
        let director = board.agents.iter().find(|a| a.is_director);
        if let Some(user) = board.agent(USER) {
            places.push((user, USER_PLACE));
        }
        if let Some(director) = director {
            places.push((director, DIRECTOR_PLACE));
            edges.push(Edge {
                a: director.id.clone(),
                b: USER.into(),
                points: vec![(480., 212.), (480., 76.)],
                label: (492., 130.),
            });
        }
        let mut workers: Vec<&Agent> = board
            .agents
            .iter()
            .filter(|a| !a.is_director && a.cli.is_some())
            .collect();
        // The reviewer goes on the right, as in the preview.
        let mut order: Vec<usize> = vec![0, 2, 3, 4];
        if let Some(index) = workers
            .iter()
            .position(|a| Some(&a.id) == board.review_role.as_ref())
        {
            let reviewer = workers.remove(index);
            workers.insert(0, reviewer);
            order = vec![1, 0, 2, 3, 4];
        }
        let slots = slots();
        for (agent, slot) in workers.iter().zip(order) {
            let (place, points, label) = slots[slot].clone();
            places.push((agent, place));
            if let Some(director) = director {
                edges.push(Edge {
                    a: director.id.clone(),
                    b: agent.id.clone(),
                    points,
                    label,
                });
            }
        }

        let live = board.live_message(now);
        let mut layer: Vec<AnyElement> = Vec::new();
        let mut labels: Vec<AnyElement> = Vec::new();
        for edge in &edges {
            let on = live.filter(|m| {
                (m.from == edge.a && m.to == edge.b) || (m.from == edge.b && m.to == edge.a)
            });
            let line = match on {
                Some(m) => hsla(agent_hex(m.sender)),
                None => {
                    let mut c = hsla(theme::INK);
                    c.a = 0.14;
                    c
                }
            };
            for segment in edge.points.windows(2) {
                let (x1, y1) = segment[0];
                let (x2, y2) = segment[1];
                layer.push(
                    div()
                        .absolute()
                        .left(px(x1.min(x2) - 1.))
                        .top(px(y1.min(y2) - 1.))
                        .w(px((x2 - x1).abs().max(2.)))
                        .h(px((y2 - y1).abs().max(2.)))
                        .bg(line)
                        .into_any_element(),
                );
            }
            if let Some(m) = on {
                let forward = m.from == edge.a;
                let fraction = ((now - m.at_ms).rem_euclid(TRIP_MS)) as f32 / TRIP_MS as f32;
                let (x, y) = along(&edge.points, if forward { fraction } else { 1. - fraction });
                let sender = hsla(agent_hex(m.sender));
                layer.push(
                    div()
                        .absolute()
                        .left(px(x - 5.))
                        .top(px(y - 5.))
                        .size(px(10.))
                        .rounded_full()
                        .bg(sender)
                        .into_any_element(),
                );
                let horizontal = edge.points.len() == 2 && edge.points[0].1 == edge.points[1].1;
                let (left, width) = if horizontal {
                    // Centred over the line, above both boxes.
                    let mid = (edge.points[0].0 + edge.points[1].0) / 2.;
                    (mid - LABEL_W / 2., LABEL_W)
                } else {
                    (edge.label.0, 260.)
                };
                labels.push(
                    div()
                        .absolute()
                        .left(px(left))
                        .top(px(edge.label.1))
                        .w(px(width))
                        .flex()
                        .when(horizontal, |d| d.justify_center())
                        .child(
                            div()
                                .max_w(px(width))
                                .px(px(6.))
                                .py(px(2.))
                                .bg(color(theme::SURFACE))
                                .border_1()
                                .border_color(sender)
                                .rounded(px(4.))
                                .text_size(px(11.))
                                .text_color(color(theme::INK))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(m.text.clone()),
                        )
                        .into_any_element(),
                );
            }
        }
        for (agent, place) in places {
            layer.push(self.graph_node(board, agent, place, cx).into_any_element());
        }
        layer.extend(labels);

        let feed: Vec<Div> = board
            .feed
            .iter()
            .rev()
            .take(4)
            .enumerate()
            .map(|(i, item)| {
                div()
                    .flex()
                    .gap(px(8.))
                    .opacity(1. - i as f32 * 0.12)
                    .child(
                        div()
                            .w(px(72.))
                            .flex_none()
                            .text_color(color(theme::STONE))
                            .child(if i == 0 {
                                ago(item.at_ms, now)
                            } else {
                                format!("hace {}", short_age(now - item.at_ms))
                            }),
                    )
                    .child(
                        div()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(color(theme::BODY))
                            .child(board.feed_text(item)),
                    )
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_grow_1()
                    .justify_center()
                    .items_center()
                    .min_h(px(0.))
                    .pt(px(24.))
                    .px(px(16.))
                    .pb(px(8.))
                    .overflow_hidden()
                    .child(
                        div()
                            .relative()
                            .flex_none()
                            .w(px(WIDTH))
                            .h(px(HEIGHT))
                            .children(layer),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap(px(24.))
                    .pt(px(12.))
                    .px(px(16.))
                    .pb(px(16.))
                    .border_t_1()
                    .border_color(hairline())
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.))
                            .gap(px(6.))
                            .child(div().text_color(color(theme::MUTE)).child("AHORA"))
                            .when(feed.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_color(color(theme::MUTE))
                                        .child("sin eventos todavía"),
                                )
                            })
                            .children(feed),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .w(px(300.))
                            .justify_end()
                            .gap(px(6.))
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
        board: &Board,
        agent: &Agent,
        place: Place,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let now = self.now_ms;
        let l = look(board, agent, now);
        let sub = if agent.is_director {
            "director · claude code".to_string()
        } else if agent.cli.is_none() {
            "usuario".to_string()
        } else {
            format!("{} · {}", agent.role, agent.model)
        };
        let action = if agent.cli.is_none() && l.dim {
            String::new()
        } else {
            l.action.clone()
        };
        let session = agent.session.clone();
        div()
            .id(SharedString::from(format!("node-{}", agent.id)))
            .absolute()
            .left(px(place.cx - place.w / 2.))
            .top(px(place.cy - place.h / 2.))
            .w(px(place.w))
            .h(px(place.h))
            .flex()
            .flex_col()
            .gap(px(4.))
            .px(px(12.))
            .py(px(8.))
            .bg(color(theme::CANVAS))
            .border_1()
            .border_color(l.border)
            .rounded(px(4.))
            .opacity(if l.dim { 0.75 } else { 1. })
            .when(agent.cli.is_some(), |d| d.cursor_pointer())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if session.is_some() {
                    this.open_agent(session.clone(), window, cx);
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .when_some(agent.cli, |d, cli| {
                        d.child(
                            div()
                                .text_color(color(theme::agent_color(cli)))
                                .child(cli.tag()),
                        )
                    })
                    .child(
                        div()
                            .flex_grow_1()
                            .text_color(color(theme::INK))
                            .font_weight(FontWeight::BOLD)
                            .child(agent.title.clone()),
                    )
                    .child(
                        div()
                            .text_color(color(l.marker_hex))
                            .child(l.marker.clone()),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(color(theme::MUTE))
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
                        .text_color(color(l.action_hex))
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
    fn dots_walk_polylines_by_length() {
        let points = [(0., 0.), (10., 0.), (10., 30.)];
        assert_eq!(along(&points, 0.), (0., 0.));
        assert_eq!(along(&points, 0.25), (10., 0.));
        assert_eq!(along(&points, 0.5), (10., 10.));
        assert_eq!(along(&points, 1.), (10., 30.));
        assert_eq!(slots().len(), 5);
        assert_eq!(short_age(4_000), "4 s");
        assert_eq!(short_age(185_000), "3 min");
    }
}
