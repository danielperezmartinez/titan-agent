//! One line in the daemon's log (its stderr) per request it receives and per
//! error it answers, so a test by hand leaves a trace of what the window
//! asked for. Never what the user types or writes: `input` and `resize` are
//! not logged (one per key or per frame), and `launch` without its brief or
//! command line.

use std::time::{SystemTime, UNIX_EPOCH};

use titan_agent_core::protocol::{Request, ServerMessage};

/// The log line of a request, or `None` for the ones not logged.
pub fn describe(request: &Request) -> Option<String> {
    Some(match request {
        Request::Hello { protocol, client } => format!("hello {client} (protocolo {protocol})"),
        Request::Subscribe => "subscribe".into(),
        Request::Attach {
            session,
            cols,
            rows,
        } => format!("attach {session} ({cols}x{rows})"),
        Request::Input { .. } | Request::Resize { .. } => return None,
        Request::Detach { terminal } => format!("detach {terminal}"),
        Request::CloseTerminal { terminal } => format!("close_terminal {terminal}"),
        Request::Stop { session } => format!("stop {session}"),
        Request::OpenFolder { session } => format!("open_folder {session}"),
        Request::Launch { token, spec, .. } => {
            format!("launch {:?} {} (token {token})", spec.agent, spec.model)
        }
        Request::Transcript { session } => format!("transcript {session}"),
        Request::UpdateStatus => "update_status".into(),
        Request::UpdateCheck => "update_check".into(),
        Request::UpdateApply { force } => format!("update_apply (force {force})"),
        Request::UpdateSettings { auto } => format!("update_settings (auto {auto})"),
        Request::UpdateDismiss => "update_dismiss".into(),
    })
}

/// The log line of an answer that is an error, or `None`.
pub fn describe_answer(answer: &ServerMessage) -> Option<String> {
    match answer {
        ServerMessage::Error { id, error } => Some(format!(
            "error {} en la petición {}: {}",
            error.code,
            id.map_or("?".to_string(), |id| id.to_string()),
            error.message
        )),
        _ => None,
    }
}

/// Writes `text` for `client` with the UTC time of day.
pub fn log(client: u64, text: &str) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
        % 86_400;
    eprintln!(
        "{:02}:{:02}:{:02}Z cliente {client}: {text}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use titan_agent_core::launch::LaunchSpec;
    use titan_agent_core::session::AgentCli;

    #[test]
    fn requests_are_described_without_what_the_user_writes() {
        let attach = Request::Attach {
            session: "cl:s1".into(),
            cols: 80,
            rows: 24,
        };
        assert_eq!(describe(&attach).unwrap(), "attach cl:s1 (80x24)");
        let input = Request::Input {
            terminal: "t1".into(),
            data: "c2VjcmV0".into(),
        };
        assert_eq!(describe(&input), None);
        let launch = Request::Launch {
            token: "k1".into(),
            spec: LaunchSpec {
                agent: AgentCli::Codex,
                cwd: "/srv/demo".into(),
                model: "gpt-6.1-sol".into(),
                effort: "high".into(),
                permission: "read-only".into(),
                brief: "private brief".into(),
            },
            command: "codex -- 'private brief'".into(),
        };
        let line = describe(&launch).unwrap();
        assert!(line.starts_with("launch Codex gpt-6.1-sol"), "{line}");
        assert!(!line.contains("private"), "{line}");
        let error = ServerMessage::error(7, "unknown_session", "no existe");
        assert_eq!(
            describe_answer(&error).unwrap(),
            "error unknown_session en la petición 7: no existe"
        );
        assert_eq!(
            describe_answer(&ServerMessage::result(7, &serde_json::json!({}))),
            None
        );
    }
}
