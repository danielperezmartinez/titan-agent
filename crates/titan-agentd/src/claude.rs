//! Claude Code sessions, from `claude agents --json` and, for the model shown
//! in the agent pill, the tail of each session's local transcript. Nothing
//! else under `~/.claude` is read (no credentials, ADR-0001).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use titan_agent_core::discovery::{
    claude_project_dir, last_model_in_transcript, parse_claude_agents,
};
use titan_agent_core::session::Session;

/// Bytes read from the end of a transcript to find the last model.
const TRANSCRIPT_TAIL: u64 = 256 * 1024;

pub struct ClaudeSource {
    command: String,
    projects_dir: Option<PathBuf>,
    /// Model per transcript, refreshed when the file changes.
    models: HashMap<PathBuf, (SystemTime, Option<String>)>,
}

impl ClaudeSource {
    pub fn new(command: String, home: Option<PathBuf>) -> ClaudeSource {
        ClaudeSource {
            command,
            projects_dir: home.map(|home| home.join(".claude").join("projects")),
            models: HashMap::new(),
        }
    }

    pub fn sessions(&mut self) -> Result<Vec<Session>, String> {
        let output = run_with_timeout(
            Command::new(&self.command).args(["agents", "--json"]),
            Duration::from_secs(15),
        )?;
        let mut sessions = parse_claude_agents(&output)?;
        for session in &mut sessions {
            let Some(full_id) = session.key.strip_prefix("cl:") else {
                continue;
            };
            if let Some(dir) = &self.projects_dir {
                let path = dir
                    .join(claude_project_dir(&session.cwd))
                    .join(format!("{full_id}.jsonl"));
                session.model = self.model(&path);
            }
        }
        Ok(sessions)
    }

    fn model(&mut self, path: &Path) -> Option<String> {
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
        if let Some((seen, model)) = self.models.get(path)
            && *seen == modified
        {
            return model.clone();
        }
        let model = read_tail(path, TRANSCRIPT_TAIL)
            .ok()
            .and_then(|tail| last_model_in_transcript(&tail));
        self.models
            .insert(path.to_path_buf(), (modified, model.clone()));
        model
    }
}

fn read_tail(path: &Path, bytes: u64) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(bytes)))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// Runs a command and returns its standard output, killing it after
/// `timeout`.
pub fn run_with_timeout(command: &mut Command, timeout: Duration) -> Result<String, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("no se pudo ejecutar: {error}"))?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let out = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let err = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("tardó demasiado".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => return Err(error.to_string()),
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        let detail = stderr.lines().next().unwrap_or("").trim().to_string();
        Err(if detail.is_empty() {
            format!("terminó con {status}")
        } else {
            detail
        })
    }
}
