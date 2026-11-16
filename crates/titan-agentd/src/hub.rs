//! Shared state of the daemon: known sessions, connected clients, the
//! terminals it keeps alive, and the team tasks and quotas it reports.
//! Launches (ADR-0002) live in the `launches` submodule.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use titan_agent_core::discovery::link_sessions;
use titan_agent_core::launch::{LaunchSpec, parse_backgrounded_id};
use titan_agent_core::protocol::{
    AttachResult, ClientMessage, Event, HelloResult, LaunchRef, LaunchResult, PROTOCOL_VERSION,
    Request, ServerMessage, SourceStatus, TerminalRef, encode_bytes, encode_line,
};
use titan_agent_core::quota::Quota;
use titan_agent_core::session::{AgentCli, Session, SessionState};
use titan_agent_core::team::run::TeamRun;

use crate::claude::run_with_timeout;
use crate::codex::{CodexClient, socket_path as codex_socket_path};

mod launches;

use launches::{Launched, plain_text, resolve_launches};

/// Terminal output kept for clients that attach later (closing and opening
/// the window again).
const HISTORY_BYTES: usize = 2 * 1024 * 1024;
/// Largest chunk sent in one `output` event.
const CHUNK_BYTES: usize = 64 * 1024;

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
    /// Tasks of the team tool next to the sessions (ADR-0010).
    teams: Vec<TeamRun>,
    /// Remaining quota of each CLI.
    quotas: Vec<Quota>,
    /// Launched TUIs that ended before their launch was confirmed, as
    /// terminal id → (exit code, last output).
    ended_early: HashMap<String, (Option<u32>, String)>,
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
                teams: Vec::new(),
                quotas: Vec::new(),
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

    /// The merged sessions, for the team log reader.
    pub fn sessions(&self) -> Vec<Session> {
        self.lock().sessions.clone()
    }

    /// Stores the team tasks found by the log reader.
    pub fn set_teams(&self, teams: Vec<TeamRun>) {
        let mut state = self.lock();
        state.teams = teams;
        broadcast_snapshot(&mut state, false);
    }

    /// Stores the quotas read from the CLIs.
    pub fn set_quotas(&self, quotas: Vec<Quota>) {
        let mut state = self.lock();
        state.quotas = quotas;
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
                    resize(t, cols, rows);
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

    fn attach(
        self: &Arc<Self>,
        client: u64,
        key: &str,
        cols: u16,
        rows: u16,
        id: Option<u64>,
    ) -> Result<(), (&'static str, String)> {
        let cols = cols.clamp(20, 1000);
        let rows = rows.clamp(5, 500);
        let mut state = self.lock();
        let existing = state
            .terminals
            .iter()
            .find(|(_, t)| t.session == key)
            .map(|(terminal, _)| terminal.clone());
        if let Some(terminal) = existing {
            let send = |state: &State, line: String| {
                if let Some(c) = state.clients.get(&client) {
                    let _ = c.tx.send(line);
                }
            };
            if let Some(id) = id {
                send(
                    &state,
                    encode_line(&ServerMessage::result(
                        id,
                        &AttachResult {
                            terminal: terminal.clone(),
                            created: false,
                        },
                    )),
                );
            }
            let history: Vec<u8> = state.terminals[&terminal].history.iter().copied().collect();
            for chunk in history.chunks(CHUNK_BYTES) {
                send(
                    &state,
                    encode_line(&ServerMessage::Event(Event::Output {
                        terminal: terminal.clone(),
                        data: encode_bytes(chunk),
                    })),
                );
            }
            let t = state.terminals.get_mut(&terminal).expect("terminal exists");
            t.viewers.insert(client);
            if t.size == (cols, rows) {
                // Same size: nudge the program so it redraws the screen.
                resize(t, cols.saturating_sub(1).max(20), rows);
                let hub = Arc::clone(self);
                let terminal = terminal.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(60));
                    if let Some(t) = hub.lock().terminals.get_mut(&terminal) {
                        resize(t, cols, rows);
                    }
                });
            } else {
                resize(t, cols, rows);
            }
            return Ok(());
        }

        let session = state
            .sessions
            .iter()
            .find(|s| s.key == key)
            .cloned()
            .ok_or(("unknown_session", "sesión desconocida".to_string()))?;
        let argv = session.attach.clone().ok_or((
            "not_attachable",
            "el CLI no permite adjuntar esta sesión".to_string(),
        ))?;
        let program = match session.agent {
            AgentCli::ClaudeCode => self.config.claude.clone(),
            AgentCli::Codex => self.config.codex.clone(),
        };
        let cwd = if Path::new(&session.cwd).is_dir() {
            PathBuf::from(&session.cwd)
        } else {
            self.config
                .home
                .clone()
                .unwrap_or_else(|| PathBuf::from("/"))
        };
        let terminal = self.spawn_terminal(
            &mut state,
            key,
            &program,
            &argv[1..],
            &cwd,
            (cols, rows),
            client,
        )?;
        if let Some(id) = id
            && let Some(c) = state.clients.get(&client)
        {
            let _ = c.tx.send(encode_line(&ServerMessage::result(
                id,
                &AttachResult {
                    terminal,
                    created: true,
                },
            )));
        }
        broadcast_snapshot(&mut state, true);
        Ok(())
    }

    /// Opens a PTY running `program args` in `cwd` as terminal of `session`,
    /// with `client` as its first viewer, and starts pumping its output.
    #[allow(clippy::too_many_arguments)]
    fn spawn_terminal(
        self: &Arc<Self>,
        state: &mut State,
        session: &str,
        program: &str,
        args: &[String],
        cwd: &Path,
        (cols, rows): (u16, u16),
        client: u64,
    ) -> Result<String, (&'static str, String)> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| ("pty_failed", error.to_string()))?;
        let mut command = CommandBuilder::new(program);
        command.args(args);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| ("spawn_failed", error.to_string()))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| ("pty_failed", error.to_string()))?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|error| ("pty_failed", error.to_string()))?;
        let (input, input_rx) = channel::<Vec<u8>>();
        std::thread::spawn(move || {
            for bytes in input_rx {
                if writer
                    .write_all(&bytes)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });

        let terminal = format!("t{}", state.next_terminal);
        state.next_terminal += 1;
        state.terminals.insert(
            terminal.clone(),
            Terminal {
                session: session.to_string(),
                killer: child.clone_killer(),
                master: pair.master,
                input,
                history: VecDeque::new(),
                viewers: HashSet::from([client]),
                size: (cols, rows),
            },
        );
        let hub = Arc::clone(self);
        let id = terminal.clone();
        std::thread::spawn(move || {
            let mut buffer = vec![0u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => hub.output(&id, &buffer[..n]),
                }
            }
            let exit_code = child.wait().ok().map(|status| status.exit_code());
            hub.closed(&id, exit_code);
        });
        Ok(terminal)
    }

    fn output(&self, terminal: &str, bytes: &[u8]) {
        let mut state = self.lock();
        let State {
            terminals, clients, ..
        } = &mut *state;
        let Some(t) = terminals.get_mut(terminal) else {
            return;
        };
        t.history.extend(bytes);
        if t.history.len() > HISTORY_BYTES {
            let excess = t.history.len() - HISTORY_BYTES;
            t.history.drain(..excess);
            // Start the kept history at a line boundary when possible.
            if let Some(newline) = t.history.iter().take(4096).position(|&b| b == b'\n') {
                t.history.drain(..=newline);
            }
        }
        let line = encode_line(&ServerMessage::Event(Event::Output {
            terminal: terminal.to_string(),
            data: encode_bytes(bytes),
        }));
        for viewer in &t.viewers {
            if let Some(c) = clients.get(viewer) {
                let _ = c.tx.send(line.clone());
            }
        }
    }

    fn closed(&self, terminal: &str, exit_code: Option<u32>) {
        let mut state = self.lock();
        let Some(t) = state.terminals.remove(terminal) else {
            return;
        };
        if state
            .launches
            .iter()
            .any(|l| l.running && l.key == t.session)
        {
            let tail: Vec<u8> = t.history.iter().copied().collect();
            state
                .ended_early
                .insert(terminal.to_string(), (exit_code, plain_text(&tail)));
        }
        let line = encode_line(&ServerMessage::Event(Event::TerminalClosed {
            terminal: terminal.to_string(),
            session: t.session.clone(),
            exit_code,
        }));
        for (id, c) in &state.clients {
            if c.subscribed || t.viewers.contains(id) {
                let _ = c.tx.send(line.clone());
            }
        }
        rebuild(&mut state);
        broadcast_snapshot(&mut state, true);
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

fn resize(t: &mut Terminal, cols: u16, rows: u16) {
    t.size = (cols, rows);
    let _ = t.master.resize(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    });
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
    resolve_launches(state, &mut sessions);
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
        teams: state.teams.clone(),
        quotas: state.quotas.clone(),
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
