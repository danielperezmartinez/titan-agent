//! Codex threads the interface asks about by key (`cx:<thread>`), listed or
//! not: the read-only transcript and the session of a thread the daemon no
//! longer lists, so «tomar el control» can still open it. Everything goes
//! through the official app-server and nothing here writes to a thread.

use std::sync::Arc;

use titan_agent_core::discovery::codex_session;
use titan_agent_core::protocol::{ServerMessage, TranscriptResult, encode_line};
use titan_agent_core::session::Session;

use super::Hub;
use crate::codex::{CodexClient, ReadError, socket_path as codex_socket_path};

/// Thread id of a Codex session key, if it is one.
fn thread_of(key: &str) -> Option<&str> {
    key.strip_prefix("cx:").filter(|thread| !thread.is_empty())
}

fn connect(codex: &str) -> Result<CodexClient, String> {
    let path = codex_socket_path(codex)?;
    CodexClient::connect(&path)
}

impl Hub {
    /// Answers a `transcript` request from its own thread, so a slow Codex
    /// does not hold up other clients.
    pub(super) fn transcript(self: &Arc<Self>, client: u64, id: Option<u64>, session: String) {
        let hub = Arc::clone(self);
        std::thread::spawn(move || {
            let answer = hub.read_transcript(&session);
            let Some(id) = id else { return };
            let message = match answer {
                Ok(result) => ServerMessage::result(id, &result),
                Err((code, text)) => ServerMessage::error(id, code, text),
            };
            if let Some(c) = hub.lock().clients.get(&client) {
                let _ = c.tx.send(encode_line(&message));
            }
        });
    }

    fn read_transcript(&self, key: &str) -> Result<TranscriptResult, (&'static str, String)> {
        let thread = thread_of(key).ok_or((
            "not_codex",
            "solo se leen hilos de Codex (cx:<id>)".to_string(),
        ))?;
        let mut client =
            connect(&self.config.codex).map_err(|text| ("codex_unavailable", text))?;
        let transcript = client.transcript(thread).map_err(|error| match error {
            ReadError::NotFound(text) => ("not_found", format!("Codex no encuentra el hilo: {text}")),
            ReadError::Codex(text) => ("codex_error", text),
        })?;
        Ok(TranscriptResult {
            session: key.to_string(),
            items: transcript.items,
            more: transcript.more,
            cwd: transcript.cwd,
        })
    }

    /// The session of a Codex thread the daemon does not list, as Codex
    /// reports it (`thread/read`), so it can be attached like a listed one.
    pub(super) fn unlisted_codex_session(&self, key: &str) -> Option<Session> {
        let thread = thread_of(key)?;
        if self.lock().sessions.iter().any(|s| s.key == key) {
            return None;
        }
        let read = connect(&self.config.codex).ok()?.thread(thread).ok()?;
        codex_session(&read, false).filter(|session| session.key == key)
    }
}
