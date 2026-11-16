//! Shared state of the daemon: known sessions, connected clients and the
//! terminals it keeps alive.
//!
//! Launches live in `hub/launch.rs` and terminals in `hub/terminals.rs`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{ChildKiller, MasterPty};
use titan_agent_core::discovery::link_sessions;
use titan_agent_core::launch::LaunchSpec;
use titan_agent_core::protocol::{
    ClientMessage, Event, HelloResult, LaunchRef, PROTOCOL_VERSION, Request, ServerMessage,
    SourceStatus, TerminalRef, encode_line,
};
use titan_agent_core::session::{AgentCli, Session, SessionState};

use crate::claude::run_with_timeout;
use crate::codex::{CodexClient, socket_path as codex_socket_path};

mod launch;
mod terminals;

#[derive(Clone)]
pub struct Config {
    pub claude: String,
    pub codex: String,
    pub poll: Duration,
    pub home: Option<PathBuf>,
}

pub struct Hub {
    pub config: Config,
    state: Mutex<State>,
}

struct Client {
    tx: Sender<String>,
    subscribed: bool,
}

struct Terminal {
    session: String,
    master: Box<dyn MasterPty + Send>,
    input: Sender<Vec<u8>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    history: VecDeque<u8>,
    viewers: HashSet<u64>,
    size: (u16, u16),
}

struct State {
    claude: Vec<Session>,
    codex: Vec<Session>,
    sources: [SourceStatus; 2],
    /// First time each session was seen in its current state.
    since: HashMap<String, (SessionState, i64)>,
    sessions: Vec<Session>,
    clients: HashMap<u64, Client>,
    terminals: HashMap<String, Terminal>,
    next_terminal: u64,
    last_snapshot: String,
    launches: Vec<Launched>,
    next_launch: u64,
    /// Every launch token seen, valid or not: a token is good for one click.
    used_tokens: HashSet<String>,
    /// Launches that succeeded recently, to refuse a repeat.
    recent: Vec<(Instant, LaunchSpec)>,
    /// Launched TUIs that ended before their launch was confirmed, as
    /// terminal id → (exit code, last output).
    ended_early: HashMap<String, (Option<u32>, String)>,
}

/// A session launched by this daemon (ADR-0002).
struct Launched {
    /// Provisional key, `launch:<n>`.
    key: String,
    spec: LaunchSpec,
    at_ms: i64,
    at: Instant,
    /// The CLI is still being run (`claude --bg` has not returned).
    running: bool,
    /// Id printed by `claude --bg`.
    cli_id: Option<String>,
    /// Keys of the sessions known when it was launched.
    known: HashSet<String>,
    /// The real session, once discovered.
    session: Option<String>,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

impl Hub {
    pub fn new(config: Config) -> Arc<Hub> {
        Arc::new(Hub {
            config,
            state: Mutex::new(State {
                claude: Vec::new(),
                codex: Vec::new(),
                sources: [
                    SourceStatus {
                        agent: AgentCli::ClaudeCode,
                        ok: false,
                        detail: Some("buscando sesiones".into()),
                    },
                    SourceStatus {
                        agent: AgentCli::Codex,
                        ok: false,
                        detail: Some("buscando sesiones".into()),
                    },
                ],
                since: HashMap::new(),
                sessions: Vec::new(),
                clients: HashMap::new(),
                terminals: HashMap::new(),
                next_terminal: 1,
                last_snapshot: String::new(),
                launches: Vec::new(),
                next_launch: 1,
                used_tokens: HashSet::new(),
                recent: Vec::new(),
                ended_early: HashMap::new(),
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Stores what a discovery source found and tells the clients if the
    /// snapshot changed.
    pub fn set_source(&self, agent: AgentCli, found: Result<Vec<Session>, String>) {
        let mut state = self.lock();
        let index = match agent {
            AgentCli::ClaudeCode => 0,
            AgentCli::Codex => 1,
        };
        match found {
            Ok(sessions) => {
                state.sources[index] = SourceStatus {
                    agent,
                    ok: true,
                    detail: None,
                };
                match agent {
                    AgentCli::ClaudeCode => state.claude = sessions,
                    AgentCli::Codex => state.codex = sessions,
                }
            }
            Err(detail) => {
                state.sources[index] = SourceStatus {
                    agent,
                    ok: false,
                    detail: Some(detail),
                };
                // Keep the last good list of a source that fails once, but
                // drop it when the source is gone (no daemon, no CLI).
                match agent {
                    AgentCli::ClaudeCode => state.claude.clear(),
                    AgentCli::Codex => state.codex.clear(),
                }
            }
        }
        rebuild(&mut state);
        broadcast_snapshot(&mut state, false);
    }

    pub fn add_client(&self, id: u64, tx: Sender<String>) {
        self.lock().clients.insert(
            id,
            Client {
                tx,
                subscribed: false,
            },
        );
    }

    pub fn remove_client(&self, id: u64) {
        let mut state = self.lock();
        state.clients.remove(&id);
        for terminal in state.terminals.values_mut() {
            terminal.viewers.remove(&id);
        }
    }

    /// Handles one request line from a client.
    pub fn handle(self: &Arc<Self>, client: u64, message: ClientMessage, hello_done: &mut bool) {
        let id = message.id;
        let reply = |state: &State, answer: ServerMessage| {
            if let Some(c) = state.clients.get(&client) {
                let _ = c.tx.send(encode_line(&answer));
            }
        };
        if !*hello_done && !matches!(message.request, Request::Hello { .. }) {
            if let Some(id) = id {
                reply(
                    &self.lock(),
                    ServerMessage::error(id, "hello_required", "el primer mensaje debe ser hello"),
                );
            }
            return;
        }
        match message.request {
            Request::Hello { protocol, .. } => {
                let state = self.lock();
                if protocol != PROTOCOL_VERSION {
                    if let Some(id) = id {
                        reply(
                            &state,
                            ServerMessage::error(
                                id,
                                "protocol_mismatch",
                                format!(
                                    "titan-agentd habla la versión {PROTOCOL_VERSION} del protocolo"
                                ),
                            ),
                        );
                    }
                    return;
                }
                *hello_done = true;
                if let Some(id) = id {
                    reply(
                        &state,
                        ServerMessage::result(
                            id,
                            &HelloResult {
                                protocol: PROTOCOL_VERSION,
                                daemon: env!("CARGO_PKG_VERSION").into(),
                            },
                        ),
                    );
                }
            }
            Request::Subscribe => {
                let mut state = self.lock();
                if let Some(c) = state.clients.get_mut(&client) {
                    c.subscribed = true;
                }
                if let Some(id) = id {
                    reply(&state, ServerMessage::result(id, &serde_json::json!({})));
                }
                let line = encode_line(&snapshot(&state));
                if let Some(c) = state.clients.get(&client) {
                    let _ = c.tx.send(line);
                }
            }
            Request::Attach {
                session,
                cols,
                rows,
            } => {
                let answer = self.attach(client, &session, cols, rows, id);
                if let (Some(id), Err((code, text))) = (id, answer) {
                    reply(&self.lock(), ServerMessage::error(id, code, text));
                }
            }
            Request::Input { terminal, data } => {
                let state = self.lock();
                if let (Some(t), Some(bytes)) = (
                    state.terminals.get(&terminal),
                    titan_agent_core::protocol::decode_bytes(&data),
                ) {
                    let _ = t.input.send(bytes);
                }
            }
            Request::Resize {
                terminal,
                cols,
                rows,
            } => {
                let mut state = self.lock();
                if let Some(t) = state.terminals.get_mut(&terminal) {
                    terminals::resize(t, cols, rows);
                }
            }
            Request::Detach { terminal } => {
                let mut state = self.lock();
                if let Some(t) = state.terminals.get_mut(&terminal) {
                    t.viewers.remove(&client);
                }
                if let Some(id) = id {
                    reply(&state, ServerMessage::result(id, &serde_json::json!({})));
                }
            }
            Request::CloseTerminal { terminal } => {
                let mut state = self.lock();
                if let Some(t) = state.terminals.get_mut(&terminal) {
                    let _ = t.killer.kill();
                }
                if let Some(id) = id {
                    reply(&state, ServerMessage::result(id, &serde_json::json!({})));
                }
            }
            Request::Stop { session } => {
                let hub = Arc::clone(self);
                std::thread::spawn(move || {
                    let answer = hub.stop(&session);
                    let state = hub.lock();
                    if let Some(id) = id {
                        let message = match answer {
                            Ok(()) => ServerMessage::result(id, &serde_json::json!({})),
                            Err(text) => ServerMessage::error(id, "stop_failed", text),
                        };
                        if let Some(c) = state.clients.get(&client) {
                            let _ = c.tx.send(encode_line(&message));
                        }
                    }
                });
            }
            Request::OpenFolder { session } => {
                let cwd = self
                    .lock()
                    .sessions
                    .iter()
                    .find(|s| s.key == session)
                    .map(|s| s.cwd.clone());
                let answer = match cwd {
                    Some(cwd) if Path::new(&cwd).is_dir() => open_folder(&cwd),
                    Some(_) => Err("la carpeta ya no existe".to_string()),
                    None => Err("sesión desconocida".to_string()),
                };
                if let Some(id) = id {
                    let state = self.lock();
                    reply(
                        &state,
                        match answer {
                            Ok(()) => ServerMessage::result(id, &serde_json::json!({})),
                            Err(text) => ServerMessage::error(id, "open_failed", text),
                        },
                    );
                }
            }
            Request::Launch {
                token,
                spec,
                command,
            } => self.launch(client, id, token, spec, command),
        }
    }

    fn stop(&self, key: &str) -> Result<(), String> {
        let session = self
            .lock()
            .sessions
            .iter()
            .find(|s| s.key == key)
            .cloned()
            .ok_or("sesión desconocida")?;
        if !session.stoppable {
            return Err("el CLI no ofrece un comando para detener esta sesión".into());
        }
        match session.agent {
            AgentCli::ClaudeCode => {
                run_with_timeout(
                    Command::new(&self.config.claude).args(["stop", &session.cli_id]),
                    Duration::from_secs(30),
                )?;
                Ok(())
            }
            AgentCli::Codex => {
                let path = codex_socket_path(&self.config.codex)?;
                let mut client = CodexClient::connect(&path)?;
                client.interrupt(&session.cli_id).map(|_| ())
            }
        }
    }
}

/// Merges both sources, links parents and stamps how long each session has
/// been in its state.
fn rebuild(state: &mut State) {
    let now = now_ms();
    let mut sessions = state.claude.clone();
    sessions.extend(state.codex.iter().cloned());
    let mut sessions = link_sessions(sessions);
    let mut since = HashMap::new();
    for session in &mut sessions {
        let stamp = match state.since.get(&session.key) {
            Some((previous, at)) if *previous == session.state => *at,
            Some(_) => now,
            // First time seen: trust the CLI's own timestamp if it has one.
            None => session
                .state_since_ms
                .or(session.started_at_ms)
                .unwrap_or(now),
        };
        session.state_since_ms = Some(stamp);
        since.insert(session.key.clone(), (session.state, stamp));
    }
    state.since = since;
    launch::resolve_launches(state, &mut sessions);
    state.sessions = sessions;
}

fn send_to(state: &State, client: u64, message: ServerMessage) {
    if let Some(c) = state.clients.get(&client) {
        let _ = c.tx.send(encode_line(&message));
    }
}

fn snapshot(state: &State) -> ServerMessage {
    let mut terminals: Vec<TerminalRef> = state
        .terminals
        .iter()
        .map(|(terminal, t)| TerminalRef {
            session: t.session.clone(),
            terminal: terminal.clone(),
        })
        .collect();
    terminals.sort_by(|a, b| a.terminal.cmp(&b.terminal));
    let launches = state
        .launches
        .iter()
        .map(|l| LaunchRef {
            launch: l.key.clone(),
            session: l.session.clone(),
        })
        .collect();
    ServerMessage::Event(Event::Sessions {
        sessions: state.sessions.clone(),
        sources: state.sources.to_vec(),
        terminals,
        launches,
    })
}

fn broadcast_snapshot(state: &mut State, force: bool) {
    let line = encode_line(&snapshot(state));
    if !force && line == state.last_snapshot {
        return;
    }
    for c in state.clients.values().filter(|c| c.subscribed) {
        let _ = c.tx.send(line.clone());
    }
    state.last_snapshot = line;
}

fn open_folder(cwd: &str) -> Result<(), String> {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(program)
        .arg(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut child| {
            std::thread::spawn(move || child.wait());
        })
        .map_err(|error| error.to_string())
}
