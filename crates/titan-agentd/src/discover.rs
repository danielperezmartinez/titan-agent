use crate::codex::{CodexRpc, control_socket};
use crate::source::{SessionSource, StopError};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use titan_agent_core::discovery::{
    claude_model_from_transcript, claude_session, codex_session, parse_claude_agents,
    parse_codex_thread, project_for,
};
use titan_agent_core::session::{AgentCli, Session};

pub trait CommandRunner: Send + 'static {
    fn run(&self, argv: &[String], timeout: Duration) -> io::Result<Output>;
}
pub struct SystemRunner;
impl CommandRunner for SystemRunner {
    fn run(&self, argv: &[String], timeout: Duration) -> io::Result<Output> {
        let mut child = Command::new(
            argv.first()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty argv"))?,
        )
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
        let deadline = Instant::now() + timeout;
        loop {
            if child.try_wait()?.is_some() {
                return child.wait_with_output();
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(io::ErrorKind::TimedOut, "command timed out"));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
pub struct SourceConfig {
    pub claude_home: Option<PathBuf>,
    pub codex_socket: Option<PathBuf>,
    pub timeout: Duration,
}
impl SourceConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME");
        Self {
            claude_home: std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| PathBuf::from(h).join(".claude"))),
            codex_socket: control_socket(
                std::env::var_os("CODEX_HOME").as_deref(),
                home.as_deref(),
            ),
            timeout: Duration::from_secs(10),
        }
    }
}
pub struct LiveSource {
    config: SourceConfig,
    runner: Box<dyn CommandRunner>,
    rpc: Option<CodexRpc>,
    retry_at: Instant,
    transcripts: std::collections::HashMap<String, PathBuf>,
}
impl LiveSource {
    pub fn new(config: SourceConfig, runner: Box<dyn CommandRunner>) -> Self {
        Self {
            config,
            runner,
            rpc: None,
            retry_at: Instant::now(),
            transcripts: Default::default(),
        }
    }
    fn claude(&mut self) -> Vec<Session> {
        let argv = vec!["claude".into(), "agents".into(), "--json".into()];
        let Ok(out) = self.runner.run(&argv, self.config.timeout) else {
            return vec![];
        };
        if !out.status.success() {
            return vec![];
        }
        let Ok(text) = std::str::from_utf8(&out.stdout) else {
            return vec![];
        };
        let Ok(agents) = parse_claude_agents(text) else {
            return vec![];
        };
        agents
            .iter()
            .filter_map(|a| {
                let project = project_for(&a.cwd, |d| Path::new(d).join(".git").exists());
                let model = a.session_id.as_ref().and_then(|id| self.model(id));
                claude_session(a, project, model)
            })
            .collect()
    }
    fn model(&mut self, id: &str) -> Option<String> {
        let home = self.config.claude_home.as_ref()?;
        let cached = self.transcripts.get(id).filter(|p| p.exists()).cloned();
        let path = cached.or_else(|| find_transcript(&home.join("projects"), id));
        let path = path?;
        self.transcripts.insert(id.to_owned(), path.clone());
        let file = std::fs::File::open(path).ok()?;
        use std::io::{Read, Seek, SeekFrom};
        let len = file.metadata().ok()?.len();
        let mut file = file;
        if len > 65536 {
            file.seek(SeekFrom::End(-65536)).ok()?;
        }
        let mut bytes = Vec::new();
        file.take(65536).read_to_end(&mut bytes).ok()?;
        claude_model_from_transcript(&String::from_utf8_lossy(&bytes))
    }
    fn codex(&mut self) -> Vec<Session> {
        let Some(path) = self.config.codex_socket.as_ref().filter(|p| p.exists()) else {
            self.rpc = None;
            return vec![];
        };
        if self.rpc.is_none() {
            if Instant::now() < self.retry_at {
                return vec![];
            }
            match CodexRpc::connect(path, self.config.timeout) {
                Ok(rpc) => self.rpc = Some(rpc),
                Err(_) => {
                    self.retry_at = Instant::now() + Duration::from_secs(10);
                    return vec![];
                }
            }
        }
        let result = self
            .rpc
            .as_mut()
            .unwrap()
            .loaded_threads(self.config.timeout);
        match result {
            Ok(threads) => threads
                .iter()
                .filter_map(|v| {
                    let t = parse_codex_thread(v)?;
                    let p = project_for(&t.cwd, |d| Path::new(d).join(".git").exists());
                    codex_session(&t, p)
                })
                .collect(),
            Err(_) => {
                self.rpc = None;
                self.retry_at = Instant::now() + Duration::from_secs(10);
                vec![]
            }
        }
    }
}
fn find_transcript(root: &Path, id: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let candidate = path.join(format!("{id}.jsonl"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}
impl SessionSource for LiveSource {
    fn poll(&mut self) -> Vec<Session> {
        let mut sessions = self.claude();
        sessions.extend(self.codex());
        sessions
    }
    fn stop(&mut self, session: &Session) -> Result<(), StopError> {
        if session.key.cli == AgentCli::Codex {
            return Err(StopError::Unsupported(
                "Codex no tiene comando oficial para detener un hilo".into(),
            ));
        }
        let Some(attach) = session.attach.as_ref() else {
            return Err(StopError::Unsupported(
                "Interactive Claude sessions cannot be stopped".into(),
            ));
        };
        let id = attach
            .last()
            .cloned()
            .unwrap_or_else(|| session.key.id.clone());
        let argv = vec!["claude".into(), "stop".into(), id];
        match self.runner.run(&argv, self.config.timeout) {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => Err(StopError::Failed(
                String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            )),
            Err(e) => Err(StopError::Failed(e.to_string())),
        }
    }
}
