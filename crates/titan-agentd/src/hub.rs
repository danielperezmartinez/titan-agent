//! Shared state of the daemon: known sessions, connected clients and the
//! terminals it keeps alive.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use titan_agent_core::discovery::link_sessions;
use titan_agent_core::launch::{
    LaunchSpec, LaunchedTui, parse_claude_bg_output, pick_launched_thread,
};
use titan_agent_core::protocol::{
    AttachResult, ClientMessage, Event, HelloResult, LaunchResult, PROTOCOL_VERSION, Request,
    ServerMessage, SourceStatus, TerminalRef, encode_bytes, encode_line, provisional_key,
};
use titan_agent_core::session::{AgentCli, Session, SessionState};

use crate::claude::run_with_timeout;
use crate::codex::{CodexClient, socket_path as codex_socket_path};

/// Terminal output kept for clients that attach later (closing and opening
/// the window again).
const HISTORY_BYTES: usize = 2 * 1024 * 1024;
/// Largest chunk sent in one `output` event.
const CHUNK_BYTES: usize = 64 * 1024;
/// Two launches of the same brief, agent and folder closer than this are
/// taken as a double click and the second one is refused.
const DUPLICATE_WINDOW_MS: i64 = 10_000;
/// A launched Codex TUI whose thread has not shown up after this long stays
/// under its provisional key.
const PENDING_WINDOW_MS: i64 = 10 * 60 * 1000;

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
    /// Requests to poll a source now instead of waiting for the next tick
    /// (after a launch), one flag per CLI.
    pokes: Mutex<[bool; 2]>,
    poked: Condvar,
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

/// A Codex TUI launched by the daemon whose thread is not known yet.
struct PendingCodex {
    terminal: String,
    cwd: String,
    brief: String,
    launched_ms: i64,
    /// Codex sessions that already existed at launch time.
    known: HashSet<String>,
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
    /// Clicks already used to launch (each click launches once).
    clicks: HashSet<String>,
    /// Recent launches, as `(fingerprint, time)`, to refuse double clicks.
    launches: Vec<(String, i64)>,
    pending_codex: Vec<PendingCodex>,
    /// Sessions launched by this daemon, as `<agent tag>:<cli id>`.
    launched: HashSet<String>,
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
                clicks: HashSet::new(),
                launches: Vec::new(),
                pending_codex: Vec::new(),
                launched: HashSet::new(),
            }),
            pokes: Mutex::new([false; 2]),
            poked: Condvar::new(),
        })
    }

    /// Waits for the next poll of a source: the poll interval, or less if a
    /// launch asked for a poll now.
    pub fn wait_poll(&self, agent: AgentCli, interval: Duration) {
        let index = source_index(agent);
        let mut pokes = self.pokes.lock().unwrap_or_else(|p| p.into_inner());
        if !pokes[index] {
            pokes = self
                .poked
                .wait_timeout_while(pokes, interval, |pokes| !pokes[index])
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        pokes[index] = false;
    }

    /// Asks a source to poll now.
    fn poke(&self, agent: AgentCli) {
        self.pokes.lock().unwrap_or_else(|p| p.into_inner())[source_index(agent)] = true;
        self.poked.notify_all();
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
        let index = source_index(agent);
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
            Request::Launch {
                spec,
                shown,
                click,
                cols,
                rows,
            } => {
                let hub = Arc::clone(self);
                std::thread::spawn(move || {
                    let answer = hub.launch(client, spec, &shown, click, cols, rows);
                    let (message, pump) = match answer {
                        Ok((result, pump)) => {
                            (id.map(|id| ServerMessage::result(id, &result)), pump)
                        }
                        Err((code, text)) => {
                            (id.map(|id| ServerMessage::error(id, code, text)), None)
                        }
                    };
                    if let Some(message) = message {
                        let state = hub.lock();
                        if let Some(c) = state.clients.get(&client) {
                            let _ = c.tx.send(encode_line(&message));
                        }
                    }
                    // Read the new terminal only once the client knows it,
                    // so none of its first output is lost.
                    if let Some((terminal, pump)) = pump {
                        hub.pump(terminal, pump);
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
        let program = self.program(session.agent);
        let cwd = if Path::new(&session.cwd).is_dir() {
            PathBuf::from(&session.cwd)
        } else {
            self.config
                .home
                .clone()
                .unwrap_or_else(|| PathBuf::from("/"))
        };
        let (terminal, pump) = spawn_pty(
            &mut state,
            client,
            key,
            &program,
            &argv[1..],
            cwd,
            cols,
            rows,
        )?;
        if let Some(id) = id
            && let Some(c) = state.clients.get(&client)
        {
            let _ = c.tx.send(encode_line(&ServerMessage::result(
                id,
                &AttachResult {
                    terminal: terminal.clone(),
                    created: true,
                },
            )));
        }
        broadcast_snapshot(&mut state, true);
        drop(state);
        self.pump(terminal, pump);
        Ok(())
    }

    fn program(&self, agent: AgentCli) -> String {
        match agent {
            AgentCli::ClaudeCode => self.config.claude.clone(),
            AgentCli::Codex => self.config.codex.clone(),
        }
    }

    /// Forwards a terminal's output to its viewers until its process ends.
    fn pump(self: &Arc<Self>, terminal: String, pump: Pump) {
        let hub = Arc::clone(self);
        let Pump {
            mut reader,
            mut child,
        } = pump;
        std::thread::spawn(move || {
            let mut buffer = vec![0u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => hub.output(&terminal, &buffer[..n]),
                }
            }
            let exit_code = child.wait().ok().map(|status| status.exit_code());
            hub.closed(&terminal, exit_code);
        });
    }

    /// Launches a new session with the official CLI, once, with the
    /// safeguards of ADR-0002:
    ///
    /// 1. the brief is the user's text, sent untouched, and never empty;
    /// 2. one click, one launch: a click id is accepted once and the same
    ///    brief is not launched twice in a row; nothing is retried;
    /// 3. the official binary runs as it is, and no CLI file is read;
    /// 4. the command the user saw must be exactly the one that runs;
    /// 5. Codex runs through its own TUI, in a terminal of the daemon.
    fn launch(
        self: &Arc<Self>,
        client: u64,
        spec: LaunchSpec,
        shown: &str,
        click: String,
        cols: u16,
        rows: u16,
    ) -> LaunchAnswer {
        spec.validate()
            .map_err(|error| (error.code(), error.message()))?;
        let home = self
            .config
            .home
            .as_ref()
            .map(|home| home.to_string_lossy().into_owned());
        let command = spec.command_line(home.as_deref());
        if shown != command {
            return Err((
                "command_mismatch",
                "el comando mostrado no coincide con el que se ejecutaría".into(),
            ));
        }
        if click.trim().is_empty() {
            return Err(("missing_click", "falta el clic del usuario".into()));
        }
        if !Path::new(&spec.cwd).is_dir() {
            return Err(("no_folder", format!("la carpeta no existe: {}", spec.cwd)));
        }
        let now = now_ms();
        {
            let mut state = self.lock();
            if !state.clicks.insert(click) {
                return Err(("duplicate_launch", "ese clic ya lanzó una sesión".into()));
            }
            state
                .launches
                .retain(|(_, at)| now - at < DUPLICATE_WINDOW_MS);
            let fingerprint = spec.fingerprint();
            if state.launches.iter().any(|(seen, _)| *seen == fingerprint) {
                return Err((
                    "duplicate_launch",
                    "ese mismo encargo se acaba de lanzar".into(),
                ));
            }
            state.launches.push((fingerprint, now));
        }
        let answer = self.run_launch(client, &spec, command, now, cols, rows);
        if answer.is_err() {
            // A launch that did not start does not block the user's next
            // click with the same brief.
            let fingerprint = spec.fingerprint();
            self.lock()
                .launches
                .retain(|(seen, _)| *seen != fingerprint);
        }
        answer
    }

    fn run_launch(
        self: &Arc<Self>,
        client: u64,
        spec: &LaunchSpec,
        command: String,
        now: i64,
        cols: u16,
        rows: u16,
    ) -> LaunchAnswer {
        let argv = spec.argv();
        let program = self.program(spec.agent);
        match spec.agent {
            AgentCli::ClaudeCode => {
                let output = run_with_timeout(
                    Command::new(&program)
                        .args(&argv[1..])
                        .current_dir(&spec.cwd),
                    Duration::from_secs(60),
                )
                .map_err(|text| ("launch_failed", text))?;
                let cli_id = parse_claude_bg_output(&output);
                if let Some(id) = &cli_id {
                    self.lock().launched.insert(format!("cl:{id}"));
                }
                self.poke(AgentCli::ClaudeCode);
                let result = LaunchResult {
                    agent: spec.agent,
                    cli_id,
                    terminal: None,
                    command,
                };
                Ok((result, None))
            }
            AgentCli::Codex => {
                let mut state = self.lock();
                let known = state.codex.iter().map(|s| s.key.clone()).collect();
                let key = provisional_key(&format!("t{}", state.next_terminal));
                let (terminal, pump) = spawn_pty(
                    &mut state,
                    client,
                    &key,
                    &program,
                    &argv[1..],
                    PathBuf::from(&spec.cwd),
                    cols.clamp(20, 1000),
                    rows.clamp(5, 500),
                )?;
                state.pending_codex.push(PendingCodex {
                    terminal: terminal.clone(),
                    cwd: spec.cwd.clone(),
                    brief: spec.brief.clone(),
                    launched_ms: now,
                    known,
                });
                broadcast_snapshot(&mut state, true);
                drop(state);
                self.poke(AgentCli::Codex);
                let result = LaunchResult {
                    agent: spec.agent,
                    cli_id: None,
                    terminal: Some(terminal.clone()),
                    command,
                };
                Ok((result, Some((terminal, pump))))
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

/// A launch's answer and, for a TUI, the terminal to start reading once the
/// client got the answer.
type LaunchAnswer = Result<(LaunchResult, Option<(String, Pump)>), (&'static str, String)>;

/// Reader and process of a terminal, handed to [`Hub::pump`].
struct Pump {
    reader: Box<dyn Read + Send>,
    child: Box<dyn Child + Send + Sync>,
}

/// Opens a PTY running `program args` in `cwd` and registers it as a
/// terminal of session `key`, viewed by `client`.
#[allow(clippy::too_many_arguments)]
fn spawn_pty(
    state: &mut State,
    client: u64,
    key: &str,
    program: &str,
    args: &[String],
    cwd: PathBuf,
    cols: u16,
    rows: u16,
) -> Result<(String, Pump), (&'static str, String)> {
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
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| ("spawn_failed", error.to_string()))?;
    drop(pair.slave);
    let reader = pair
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
            session: key.to_string(),
            killer: child.clone_killer(),
            master: pair.master,
            input,
            history: VecDeque::new(),
            viewers: HashSet::from([client]),
            size: (cols, rows),
        },
    );
    Ok((terminal, Pump { reader, child }))
}

fn source_index(agent: AgentCli) -> usize {
    match agent {
        AgentCli::ClaudeCode => 0,
        AgentCli::Codex => 1,
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
    state.sessions = sessions;
    bind_launched(state, now);
    let State {
        sessions, launched, ..
    } = state;
    for session in sessions.iter_mut() {
        session.launched_here =
            launched.contains(&format!("{}:{}", session.agent.tag(), session.cli_id));
    }
}

/// Moves each launched Codex TUI from its provisional key to the thread it
/// started: a thread that did not exist at launch, in the same folder, and
/// whose first words match the brief when several qualify.
fn bind_launched(state: &mut State, now: i64) {
    let State {
        pending_codex,
        codex,
        terminals,
        launched,
        ..
    } = state;
    pending_codex.retain(|pending| {
        if !terminals.contains_key(&pending.terminal)
            || now - pending.launched_ms > PENDING_WINDOW_MS
        {
            return false;
        }
        let bound: HashSet<String> = terminals.values().map(|t| t.session.clone()).collect();
        let tui = LaunchedTui {
            cwd: pending.cwd.clone(),
            brief: pending.brief.clone(),
            launched_ms: pending.launched_ms,
        };
        let taken = |key: &str| pending.known.contains(key) || bound.contains(key);
        match pick_launched_thread(&tui, codex, &taken) {
            Some(session) => {
                launched.insert(format!("cx:{}", session.cli_id));
                if let Some(t) = terminals.get_mut(&pending.terminal) {
                    t.session = session.key.clone();
                }
                false
            }
            None => true,
        }
    });
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
    ServerMessage::Event(Event::Sessions {
        sessions: state.sessions.clone(),
        sources: state.sources.to_vec(),
        terminals,
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
