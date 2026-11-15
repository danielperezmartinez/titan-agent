use crate::codex::{CodexRpc, control_socket};
use crate::source::{SessionSource, StopError};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
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
        let mut command = Command::new(
            argv.first()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty argv"))?,
        );
        command
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.process_group(0);
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let out_tx = tx.clone();
        let out_reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            let result =
                std::io::Read::read_to_end(&mut std::io::BufReader::new(stdout), &mut bytes);
            let _ = out_tx.send((0, result.map(|_| bytes)));
        });
        let err_reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            let result =
                std::io::Read::read_to_end(&mut std::io::BufReader::new(stderr), &mut bytes);
            let _ = tx.send((1, result.map(|_| bytes)));
        });
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = child.try_wait()? {
                let mut streams: [Option<Vec<u8>>; 2] = [None, None];
                for _ in 0..2 {
                    let remain = deadline.saturating_duration_since(Instant::now());
                    match rx.recv_timeout(remain) {
                        Ok((i, Ok(bytes))) => streams[i] = Some(bytes),
                        Ok((_, Err(e))) => return Err(e),
                        Err(_) => {
                            let _ = rustix::process::kill_process_group(
                                rustix::process::Pid::from_raw(child.id() as i32).unwrap(),
                                rustix::process::Signal::KILL,
                            );
                            drop(out_reader);
                            drop(err_reader);
                            return Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "command capture timed out",
                            ));
                        }
                    }
                }
                let _ = out_reader.join();
                let _ = err_reader.join();
                return Ok(Output {
                    status,
                    stdout: streams[0].take().unwrap_or_default(),
                    stderr: streams[1].take().unwrap_or_default(),
                });
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = rustix::process::kill_process_group(
                    rustix::process::Pid::from_raw(child.id() as i32).unwrap(),
                    rustix::process::Signal::KILL,
                );
                drop(out_reader);
                drop(err_reader);
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
        if !valid_session_id(id) {
            return None;
        }
        let home = self.config.claude_home.as_ref()?;
        let cached = self
            .transcripts
            .get(id)
            .filter(|p| safe_transcript(p))
            .cloned();
        let path = cached.or_else(|| find_transcript(&home.join("projects"), id));
        let path = path?;
        self.transcripts.insert(id.to_owned(), path.clone());
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(path)
            .ok()?;
        use std::io::{Read, Seek, SeekFrom};
        if !file.metadata().ok()?.is_file() {
            return None;
        }
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
    if !valid_session_id(id) {
        return None;
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
        {
            let candidate = path.join(format!("{id}.jsonl"));
            if safe_transcript(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}
fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn safe_transcript(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
        && path
            .parent()
            .and_then(|p| std::fs::symlink_metadata(p).ok())
            .is_some_and(|m| m.is_dir() && !m.file_type().is_symlink())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_ids_are_plain_file_names_and_symlinks_are_rejected() {
        assert!(valid_session_id("session-123"));
        for id in ["", ".", "..", "a/b", "a\0b", "with space"] {
            assert!(!valid_session_id(id), "accepted {id:?}");
        }
        let root = std::env::temp_dir().join(format!("titan-transcript-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("projects").join("project");
        std::fs::create_dir_all(&project).unwrap();
        let file = project.join("session-123.jsonl");
        std::fs::write(&file, "{}\n").unwrap();
        assert_eq!(
            find_transcript(&root.join("projects"), "session-123"),
            Some(file.clone())
        );
        let link = project.join("linked.jsonl");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(!safe_transcript(&link));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn system_runner_drains_large_output_and_bounds_capture_wait() {
        let output = SystemRunner
            .run(
                &["sh".into(), "-c".into(), "head -c 300000 /dev/zero".into()],
                Duration::from_secs(3),
            )
            .unwrap();
        assert_eq!(output.stdout.len(), 300000);
        let started = Instant::now();
        let result = SystemRunner.run(
            &["sh".into(), "-c".into(), "(sleep 3) & exit 0".into()],
            Duration::from_millis(100),
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
