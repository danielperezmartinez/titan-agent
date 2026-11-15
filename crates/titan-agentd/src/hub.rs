//! Shared state of the daemon: known sessions, connected clients and the
//! terminals it keeps alive.

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
use titan_agent_core::session::{AgentCli, Session, SessionState};

use crate::claude::run_with_timeout;
use crate::codex::{CodexClient, socket_path as codex_socket_path};

/// Terminal output kept for clients that attach later (closing and opening
/// the window again).
const HISTORY_BYTES: usize = 2 * 1024 * 1024;
/// Largest chunk sent in one `output` event.
const CHUNK_BYTES: usize = 64 * 1024;
/// The same agent, folder and brief cannot be launched again within this
/// time (a double click), whatever the token says.
const REPEAT_WINDOW: Duration = Duration::from_secs(30);
/// Longest wait for `claude --bg` to return.
const CLAUDE_LAUNCH_TIMEOUT: Duration = Duration::from_secs(60);
/// How long the provisional session of a Claude Code launch stays in the
/// tree if the CLI never reports the real one.
const PROVISIONAL_TTL: Duration = Duration::from_secs(120);
/// Terminal size of a launched TUI until the interface resizes it.
const LAUNCH_SIZE: (u16, u16) = (120, 40);

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

    /// Handles a `launch` request: checks every safeguard of ADR-0002, then
    /// runs the official CLI once. Answers the client itself.
    fn launch(
        self: &Arc<Self>,
        client: u64,
        id: Option<u64>,
        token: String,
        spec: LaunchSpec,
        command: String,
    ) {
        let mut state = self.lock();
        let key = match begin_launch(&mut state, token, &spec, &command) {
            Ok(key) => key,
            Err((code, text)) => {
                if let Some(id) = id {
                    send_to(&state, client, ServerMessage::error(id, code, text));
                }
                return;
            }
        };
        rebuild(&mut state);
        broadcast_snapshot(&mut state, true);
        let argv = spec.argv();
        match spec.agent {
            AgentCli::Codex => {
                let program = self.config.codex.clone();
                let spawned = self.spawn_terminal(
                    &mut state,
                    &key,
                    &program,
                    &argv[1..],
                    Path::new(&spec.cwd),
                    LAUNCH_SIZE,
                    client,
                );
                let answer = spawned.map(|terminal| LaunchResult {
                    launch: key.clone(),
                    agent: spec.agent,
                    terminal: Some(terminal),
                    cli_id: None,
                });
                finish_launch(&mut state, &key, answer.as_ref().ok().map(|_| None));
                if let Some(id) = id {
                    send_to(&state, client, launch_answer(id, answer));
                }
            }
            AgentCli::ClaudeCode => {
                drop(state);
                let hub = Arc::clone(self);
                std::thread::spawn(move || {
                    let program = hub.config.claude.clone();
                    let ran = run_once(&program, &argv[1..], &spec.cwd, CLAUDE_LAUNCH_TIMEOUT);
                    let answer = ran.map(|output| LaunchResult {
                        launch: key.clone(),
                        agent: spec.agent,
                        terminal: None,
                        cli_id: parse_backgrounded_id(&output),
                    });
                    let mut state = hub.lock();
                    let cli_id = answer.as_ref().ok().map(|r| r.cli_id.clone());
                    finish_launch(&mut state, &key, cli_id);
                    if let Some(id) = id {
                        send_to(&state, client, launch_answer(id, answer));
                    }
                });
            }
        }
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

/// Links each launch to the real session once the CLI reports it, moves its
/// terminal there, and shows a provisional session until then.
fn resolve_launches(state: &mut State, sessions: &mut Vec<Session>) {
    let State {
        launches,
        terminals,
        ..
    } = state;
    let mut claimed: HashSet<String> = launches.iter().filter_map(|l| l.session.clone()).collect();
    let mut provisional = Vec::new();
    for launch in launches.iter_mut() {
        if launch.session.is_none() {
            let found = match launch.spec.agent {
                AgentCli::ClaudeCode => launch.cli_id.as_ref().and_then(|cli_id| {
                    sessions
                        .iter()
                        .find(|s| s.agent == AgentCli::ClaudeCode && &s.cli_id == cli_id)
                }),
                AgentCli::Codex => sessions
                    .iter()
                    .filter(|s| {
                        !launch.known.contains(&s.key)
                            && !claimed.contains(&s.key)
                            && launch.spec.matches_new_session(s, launch.at_ms)
                    })
                    .min_by_key(|s| s.started_at_ms.unwrap_or(i64::MAX)),
            };
            if let Some(session) = found {
                launch.session = Some(session.key.clone());
                claimed.insert(session.key.clone());
            }
        }
        let has_terminal = |key: &str| terminals.values().any(|t| t.session == key);
        match &launch.session {
            Some(real) => {
                for t in terminals.values_mut().filter(|t| t.session == launch.key) {
                    t.session = real.clone();
                }
            }
            None => {
                let alive = match launch.spec.agent {
                    AgentCli::ClaudeCode => launch.running || launch.at.elapsed() < PROVISIONAL_TTL,
                    AgentCli::Codex => launch.running || has_terminal(&launch.key),
                };
                if alive {
                    provisional.push(launch.spec.provisional_session(&launch.key, launch.at_ms));
                }
            }
        }
    }
    // A Codex TUI launched here runs its own app-server, so the shared one
    // reports its thread as not loaded ("finished") while it is open in our
    // terminal: show it as live instead.
    for launch in launches.iter() {
        if launch.spec.agent != AgentCli::Codex {
            continue;
        }
        let Some(real) = &launch.session else {
            continue;
        };
        let open = terminals.values().any(|t| &t.session == real);
        if let Some(session) = sessions.iter_mut().find(|s| &s.key == real)
            && open
            && session.state == (SessionState::Finished { failed: false })
        {
            session.state = SessionState::Waiting;
        }
    }
    sessions.extend(provisional);
}

/// Checks a launch request against the safeguards, in the documented order,
/// and registers it. Returns its provisional key.
fn begin_launch(
    state: &mut State,
    token: String,
    spec: &LaunchSpec,
    command: &str,
) -> Result<String, (&'static str, String)> {
    // A token is good for one click, even if that click fails.
    let token_reused = !state.used_tokens.insert(token);
    spec.validate()
        .map_err(|error| (error.code(), error.to_string()))?;
    if !Path::new(&spec.cwd).is_dir() {
        return Err(("bad_folder", "la carpeta de trabajo no existe".into()));
    }
    if command != spec.command_line() {
        return Err((
            "command_mismatch",
            "el comando no es el que muestra el formulario".into(),
        ));
    }
    if token_reused {
        return Err(("duplicate_launch", "este clic ya lanzó una sesión".into()));
    }
    state.recent.retain(|(at, _)| at.elapsed() < REPEAT_WINDOW);
    let same = |other: &LaunchSpec| {
        other.agent == spec.agent && other.cwd == spec.cwd && other.brief == spec.brief
    };
    let repeated = state.recent.iter().any(|(_, other)| same(other))
        || state.launches.iter().any(|l| l.running && same(&l.spec));
    if repeated {
        return Err((
            "duplicate_launch",
            "ya se lanzó este mismo encargo hace un momento".into(),
        ));
    }
    let key = format!("launch:{}", state.next_launch);
    state.next_launch += 1;
    let known = state.sessions.iter().map(|s| s.key.clone()).collect();
    state.launches.push(Launched {
        key: key.clone(),
        spec: spec.clone(),
        at_ms: now_ms(),
        at: Instant::now(),
        running: true,
        cli_id: None,
        known,
        session: None,
    });
    Ok(key)
}

/// Records how a launch ended: `Some(cli_id)` on success, `None` on failure
/// (the launch is forgotten, its token stays used).
fn finish_launch(state: &mut State, key: &str, outcome: Option<Option<String>>) {
    match outcome {
        Some(cli_id) => {
            if let Some(launch) = state.launches.iter_mut().find(|l| l.key == key) {
                launch.running = false;
                launch.cli_id = cli_id;
                let spec = launch.spec.clone();
                state.recent.push((Instant::now(), spec));
            }
        }
        None => state.launches.retain(|l| l.key != key),
    }
    rebuild(state);
    broadcast_snapshot(state, true);
}

/// Spanish explanation of a failed launch, followed by what the CLI said
/// (its first lines, untranslated).
fn failure_message(output: &str, status: &str) -> String {
    let said: Vec<&str> = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(3)
        .collect();
    if said.is_empty() {
        return format!("el CLI terminó con {status} sin decir por qué");
    }
    let said = said.join(" · ");
    if said.contains("Workspace not trusted") {
        format!(
            "Claude Code aún no confía en esta carpeta: abre `claude` una vez en ella, acepta el aviso y vuelve a lanzar. (CLI: {said})"
        )
    } else {
        format!("el CLI no lanzó la sesión: {said}")
    }
}

fn launch_answer(id: u64, answer: Result<LaunchResult, (&'static str, String)>) -> ServerMessage {
    match answer {
        Ok(result) => ServerMessage::result(id, &result),
        Err((code, text)) => {
            let code = if code == "launch_failed" {
                code
            } else {
                "launch_failed"
            };
            ServerMessage::error(id, code, text)
        }
    }
}

fn send_to(state: &State, client: u64, message: ServerMessage) {
    if let Some(c) = state.clients.get(&client) {
        let _ = c.tx.send(encode_line(&message));
    }
}

/// Runs a launch command once, without a shell, and returns its output. Its
/// output goes to temporary files, not pipes: `claude --bg` leaves a
/// background process behind that could keep a pipe open.
fn run_once(
    program: &str,
    args: &[String],
    cwd: &str,
    timeout: Duration,
) -> Result<String, (&'static str, String)> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("titan-agentd-launch-{}-{n}", std::process::id()));
    let failed = |text: String| ("launch_failed", text);
    let out_path = base.with_extension("out");
    let out = std::fs::File::create(&out_path).map_err(|e| failed(e.to_string()))?;
    let err = out.try_clone().map_err(|e| failed(e.to_string()))?;
    let spawned = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn();
    let result = (|| {
        let mut child =
            spawned.map_err(|e| failed(format!("no se pudo ejecutar {program}: {e}")))?;
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if start.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(failed(format!(
                        "{program} no terminó en {} s",
                        timeout.as_secs()
                    )));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(e) => return Err(failed(e.to_string())),
            }
        };
        let output = std::fs::read_to_string(&out_path).unwrap_or_default();
        if status.success() {
            Ok(output)
        } else {
            Err(failed(failure_message(&output, &status.to_string())))
        }
    })();
    let _ = std::fs::remove_file(&out_path);
    result
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

#[cfg(test)]
mod tests {
    use super::*;
    use titan_agent_core::launch::LaunchSpec;

    fn hub() -> Arc<Hub> {
        Hub::new(Config {
            claude: "claude".into(),
            codex: "codex".into(),
            poll: Duration::from_secs(1),
            home: None,
        })
    }

    fn codex_thread(key: &str, cwd: &str, started_at_ms: i64) -> Session {
        LaunchSpec {
            agent: AgentCli::Codex,
            cwd: cwd.into(),
            model: "gpt-6-luna".into(),
            effort: "low".into(),
            permission: "read-only".into(),
            brief: "x".into(),
        }
        .provisional_session(key, started_at_ms)
    }

    #[test]
    fn failures_are_explained_in_spanish_with_the_cli_text() {
        let trust = failure_message("Workspace not trusted. Run `claude` in /x once.\n", "1");
        assert!(trust.starts_with("Claude Code aún no confía"));
        assert!(trust.contains("Workspace not trusted"));
        assert_eq!(
            failure_message("\n boom \n", "1"),
            "el CLI no lanzó la sesión: boom"
        );
        assert!(failure_message("", "exit status: 3").contains("exit status: 3"));
    }

    #[test]
    fn codex_launch_resolves_to_the_new_thread_in_its_folder() {
        let hub = hub();
        let old = codex_thread("cx:old", "/srv/demo/kite", now_ms() - 60_000);
        hub.set_source(AgentCli::Codex, Ok(vec![old.clone()]));
        let spec = LaunchSpec {
            agent: AgentCli::Codex,
            cwd: "/srv/demo/kite".into(),
            model: "gpt-6-luna".into(),
            effort: "low".into(),
            permission: "read-only".into(),
            brief: "nuevo".into(),
        };
        {
            let mut state = hub.lock();
            state.launches.push(Launched {
                key: "launch:1".into(),
                spec,
                at_ms: now_ms(),
                at: Instant::now(),
                running: true,
                cli_id: None,
                known: HashSet::from(["cx:old".to_string()]),
                session: None,
            });
            rebuild(&mut state);
            assert!(state.sessions.iter().any(|s| s.key == "launch:1"));
        }
        // A thread in another folder and the old one do not match.
        let other = codex_thread("cx:other", "/srv/demo/else", now_ms());
        let new = codex_thread("cx:new", "/srv/demo/kite/", now_ms());
        hub.set_source(AgentCli::Codex, Ok(vec![old, other, new]));
        let state = hub.lock();
        assert_eq!(state.launches[0].session.as_deref(), Some("cx:new"));
        assert!(!state.sessions.iter().any(|s| s.key == "launch:1"));
    }

    #[test]
    fn launched_tui_thread_is_live_while_its_terminal_is_open() {
        let hub = hub();
        let mut thread = codex_thread("cx:new", "/srv/demo/kite", now_ms());
        thread.state = SessionState::Finished { failed: false };
        {
            let mut state = hub.lock();
            state.launches.push(Launched {
                key: "launch:1".into(),
                spec: LaunchSpec {
                    agent: AgentCli::Codex,
                    cwd: "/srv/demo/kite".into(),
                    model: "gpt-6-luna".into(),
                    effort: "low".into(),
                    permission: "read-only".into(),
                    brief: "x".into(),
                },
                at_ms: now_ms(),
                at: Instant::now(),
                running: false,
                cli_id: None,
                known: HashSet::new(),
                session: Some("cx:new".into()),
            });
        }
        hub.set_source(AgentCli::Codex, Ok(vec![thread.clone()]));
        // No terminal: the CLI's state stands.
        assert_eq!(
            hub.lock().sessions[0].state,
            SessionState::Finished { failed: false }
        );
        let mut state = hub.lock();
        let pair = native_pty_system()
            .openpty(PtySize::default())
            .expect("a pty for the test");
        let child = pair
            .slave
            .spawn_command(CommandBuilder::new("sleep"))
            .expect("spawn sleep");
        state.terminals.insert(
            "t1".into(),
            Terminal {
                session: "cx:new".into(),
                killer: child.clone_killer(),
                master: pair.master,
                input: channel().0,
                history: VecDeque::new(),
                viewers: HashSet::new(),
                size: (80, 24),
            },
        );
        rebuild(&mut state);
        assert_eq!(state.sessions[0].state, SessionState::Waiting);
        let _ = state.terminals.get_mut("t1").map(|t| t.killer.kill());
    }
}
