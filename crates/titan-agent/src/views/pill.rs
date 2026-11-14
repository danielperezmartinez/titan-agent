use crate::theme;
use gpui::{IntoElement, Styled, div, prelude::*, px, rgb};
use titan_agent_core::session::AgentCli;
pub fn agent_pill(cli: AgentCli, model: Option<&str>) -> impl IntoElement {
    let label = match model.filter(|s| !s.is_empty()) {
        Some(model) => format!("{} {model}", cli.tag()),
        None => cli.tag().to_owned(),
    };
    let color = rgb(theme::agent_color(cli));
    div()
        .border_1()
        .border_color(color)
        .rounded(px(4.))
        .px_1()
        .text_color(color)
        .child(label)
}
