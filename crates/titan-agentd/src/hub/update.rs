//! The `update_*` requests (ADR-0011 §6) and what the updater needs from
//! the hub: how many windows are connected, which sessions would lose their
//! terminal, the `update` event and closing every terminal before `exec`.

use super::*;
use titan_agent_core::protocol::UpdateStatus;
use titan_agent_core::update::apply_blockers;

impl Hub {
    pub(super) fn update_request(self: &Arc<Self>, client: u64, id: Option<u64>, request: Request) {
        let updater = &self.updater;
        let answer: Result<Option<UpdateStatus>, (&'static str, String)> = match request {
            Request::UpdateStatus => Ok(Some(updater.status(self))),
            Request::UpdateCheck if !updater.is_active() => Err(crate::update::inactive_error()),
            Request::UpdateCheck => {
                updater.request_check();
                Ok(Some(updater.status(self)))
            }
            Request::UpdateApply { force } => updater.apply_now(self, force).map(|()| None),
            Request::UpdateSettings { auto } => updater
                .set_auto(self, auto)
                .map(|()| Some(updater.status(self)))
                .map_err(|text| ("settings_failed", text)),
            Request::UpdateDismiss => {
                updater.dismiss(self);
                Ok(None)
            }
            _ => return,
        };
        if let Some(id) = id {
            let message = match answer {
                Ok(Some(status)) => ServerMessage::result(id, &status),
                Ok(None) => ServerMessage::result(id, &serde_json::json!({})),
                Err((code, text)) => ServerMessage::error(id, code, text),
            };
            send_to(&self.lock(), client, message);
        }
    }

    /// Connected windows (clients past `hello`) and the sessions that
    /// block applying alone.
    pub fn windows_and_blockers(&self) -> (usize, Vec<String>) {
        let state = self.lock();
        let windows = state.clients.values().filter(|c| c.hello).count();
        let terminals: Vec<TerminalRef> = state
            .terminals
            .iter()
            .map(|(terminal, t)| TerminalRef {
                session: t.session.clone(),
                terminal: terminal.clone(),
            })
            .collect();
        (windows, apply_blockers(&state.sessions, &terminals))
    }

    /// Sends the `update` event to every subscribed client.
    pub fn broadcast_update(&self, status: UpdateStatus) {
        let line = encode_line(&ServerMessage::Event(Event::Update(status)));
        let state = self.lock();
        for c in state.clients.values().filter(|c| c.subscribed) {
            let _ = c.tx.send(line.clone());
        }
    }

    /// Ends every terminal of the daemon (the sessions stay alive in their
    /// CLI), before the daemon replaces itself.
    pub fn close_terminals(&self) {
        let mut state = self.lock();
        for terminal in state.terminals.values_mut() {
            let _ = terminal.killer.kill();
        }
    }
}
