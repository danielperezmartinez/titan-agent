//! Pure presentation decisions for the main window.

use crate::theme;
use titan_agent_core::{
    discovery::display_path,
    session::{AgentCli, Project, Session, SessionKey, SessionState, StateCounts},
    tree::since_label,
};

/// How a state is shown: ASCII marker, Spanish label, and color.
pub fn state_style(state: SessionState) -> (&'static str, &'static str, u32) {
    match state {
        SessionState::Working => ("[>]", "trabajando", theme::SUCCESS),
        SessionState::NeedsYou => ("[?]", "te necesita", theme::WARNING),
        SessionState::Waiting => ("[+]", "en espera", theme::MUTE),
        SessionState::Finished { failed: false } => ("[x]", "terminada", theme::MUTE),
        SessionState::Finished { failed: true } => ("[x]", "terminada con error", theme::DANGER),
    }
}

/// Color associated with an agent CLI.
pub fn agent_color(cli: AgentCli) -> u32 {
    match cli {
        AgentCli::ClaudeCode => theme::CLAUDE,
        AgentCli::Codex => theme::CODEX,
    }
}

/// Agent pill contents: tag and optional model.
pub fn pill(session: &Session) -> (&'static str, String) {
    (
        session.key.cli.tag(),
        session.model.clone().unwrap_or_default(),
    )
}

/// Agent line shown in a session bar.
pub fn agent_line(session: &Session) -> String {
    match session.model.as_deref() {
        Some(model) => format!("{} · {model}", session.key.cli),
        None => session.key.cli.to_string(),
    }
}

/// Attach command line shown in a session bar.
pub fn attach_line(session: &Session) -> Option<String> {
    session
        .attach
        .as_ref()
        .map(|argv| format!("$ {}", argv.join(" ")))
}

/// Counts live sessions in the fixed header order.
pub fn header_counts(sessions: &[Session]) -> [(&'static str, String, u32); 3] {
    let counts = StateCounts::tally(sessions.iter().map(|session| session.state));
    [
        (
            "[>]",
            format!("{} trabajando", counts.working),
            theme::SUCCESS,
        ),
        (
            "[?]",
            format!("{} te necesita", counts.needs_you),
            theme::WARNING,
        ),
        ("[+]", format!("{} en espera", counts.waiting), theme::MUTE),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Talk {
    pub direction: &'static str,
    pub who: String,
    pub color: u32,
    pub what: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    pub project: String,
    pub role: &'static str,
    pub launched_by: String,
    pub since: String,
    pub folder: String,
    pub talks: Vec<Talk>,
}

fn model_or_tag(session: &Session) -> &str {
    session
        .model
        .as_deref()
        .unwrap_or_else(|| session.key.cli.tag())
}

fn talk(session: &Session, direction: &'static str) -> Talk {
    let (marker, label, _) = state_style(session.state);
    Talk {
        direction,
        who: format!("{} · {}", model_or_tag(session), session.name),
        color: agent_color(session.key.cli),
        what: format!("{marker} {label}"),
    }
}

/// Builds the detail panel for a selected session.
pub fn detail(
    sessions: &[Session],
    key: &SessionKey,
    now_ms: i64,
    home: Option<&str>,
) -> Option<Detail> {
    let selected = sessions.iter().find(|session| &session.key == key)?;
    let parent = selected
        .parent
        .as_ref()
        .and_then(|parent_key| sessions.iter().find(|session| &session.key == parent_key));
    let mut children: Vec<&Session> = sessions
        .iter()
        .filter(|session| session.parent.as_ref() == Some(key))
        .collect();
    children.sort_by(|a, b| {
        a.started_at_ms
            .is_none()
            .cmp(&b.started_at_ms.is_none())
            .then_with(|| a.started_at_ms.cmp(&b.started_at_ms))
            .then_with(|| a.name.cmp(&b.name))
    });
    let has_children = !children.is_empty();

    let mut talks = Vec::with_capacity(children.len() + usize::from(parent.is_some()));
    if let Some(parent) = parent {
        talks.push(talk(parent, "lanzada por"));
    }
    talks.extend(children.into_iter().map(|child| talk(child, "lanzó a")));

    let role = if has_children {
        "director"
    } else if selected.parent.is_some() {
        "encargada"
    } else {
        "sesión suelta"
    };
    Some(Detail {
        project: selected.project.name.clone(),
        role,
        launched_by: parent
            .map(|session| format!("{} ({})", session.name, model_or_tag(session)))
            .unwrap_or_else(|| "tú".into()),
        since: selected
            .state_since_ms
            .map(|since| since_label(now_ms, since))
            .unwrap_or_else(|| "—".into()),
        folder: display_path(&selected.cwd, home),
        talks,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopAction {
    StopSession,
    CloseTerminal,
    Disabled,
}

/// Determines what the stop button does for a session.
pub fn stop_action(session: &Session, terminal_open: bool) -> StopAction {
    if !session.state.is_live() {
        return StopAction::Disabled;
    }
    match session.key.cli {
        AgentCli::ClaudeCode if session.attach.is_some() => StopAction::StopSession,
        AgentCli::Codex if terminal_open => StopAction::CloseTerminal,
        _ => StopAction::Disabled,
    }
}

fn grid_axis(size: f32, padding: f32, cell: f32) -> u16 {
    if !size.is_finite() || !padding.is_finite() || !cell.is_finite() || cell <= 0.0 {
        return 1;
    }
    let available = size - 2.0 * padding;
    if !available.is_finite() || available <= 0.0 {
        return 1;
    }
    (available / cell).floor().clamp(1.0, 1000.0) as u16
}

/// Computes a terminal grid, clamped to a usable range on each axis.
pub fn grid_size(
    width: f32,
    height: f32,
    padding: f32,
    cell_width: f32,
    line_height: f32,
) -> (u16, u16) {
    (
        grid_axis(width, padding, cell_width),
        grid_axis(height, padding, line_height),
    )
}

/// Text shown when a session cannot be attached.
pub fn not_attachable_text(cli: AgentCli) -> (&'static str, &'static str) {
    match cli {
        AgentCli::ClaudeCode => (
            "Sesión interactiva de Claude Code",
            "Está abierta en otro terminal. titan-agent solo puede adjuntarse a sesiones en segundo plano (claude --bg).",
        ),
        AgentCli::Codex => (
            "Sesión de Codex sin terminal",
            "Codex no ofrece un comando para adjuntarse a esta sesión.",
        ),
    }
}

/// Placeholder copy for a project's future live monitor.
pub fn monitor_text(sessions: &[Session], project: &Project) -> (String, &'static str, String) {
    let members: Vec<&Session> = sessions
        .iter()
        .filter(|session| &session.project == project)
        .collect();
    let has_team = members.iter().any(|session| {
        session
            .parent
            .as_ref()
            .is_some_and(|parent| members.iter().any(|candidate| &candidate.key == parent))
    });
    let names = members
        .iter()
        .map(|session| session.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if has_team {
        (
            format!("Equipo trabajando en {}", project.name),
            "El monitor en tiempo real llega en la fase 3.",
            format!("Sesiones: {names}"),
        )
    } else {
        (
            format!("Sin equipo trabajando en {}", project.name),
            "El monitor se anima cuando las sesiones del proyecto se encargan trabajo entre ellas.",
            format!("Sesiones sueltas: {names}"),
        )
    }
}
