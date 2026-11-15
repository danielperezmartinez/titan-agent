use std::path::Path;
use titan_agent_core::session::{Session, SessionState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopError {
    Unsupported(String),
    Failed(String),
}

pub trait SessionSource: Send + 'static {
    fn poll(&mut self) -> Vec<Session>;
    fn stop(&mut self, _session: &Session) -> Result<(), StopError> {
        Err(StopError::Unsupported(
            "session stopping is unsupported".into(),
        ))
    }
}

pub struct FixtureSource {
    sessions: Vec<Session>,
}
impl FixtureSource {
    pub fn new(sessions: Vec<Session>) -> FixtureSource {
        FixtureSource { sessions }
    }
    pub fn from_file(path: &Path) -> std::io::Result<FixtureSource> {
        let data = std::fs::read(path)?;
        let sessions = serde_json::from_slice(&data).map_err(std::io::Error::other)?;
        Ok(Self::new(sessions))
    }
}
impl SessionSource for FixtureSource {
    fn poll(&mut self) -> Vec<Session> {
        self.sessions.clone()
    }
    fn stop(&mut self, session: &Session) -> Result<(), StopError> {
        if let Some(found) = self
            .sessions
            .iter_mut()
            .find(|item| item.key == session.key)
        {
            found.state = SessionState::Finished { failed: false };
            Ok(())
        } else {
            Err(StopError::Failed("session disappeared".into()))
        }
    }
}

pub struct EmptySource;
impl SessionSource for EmptySource {
    fn poll(&mut self) -> Vec<Session> {
        Vec::new()
    }
}
