//! PTY attachment, terminal output handling and terminal resizing.

use super::*;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::mpsc::channel;
use titan_agent_core::protocol::{AttachResult, encode_bytes};

/// Terminal output kept for clients that attach later (closing and opening
/// the window again).
const HISTORY_BYTES: usize = 2 * 1024 * 1024;
/// Largest chunk sent in one `output` event.
const CHUNK_BYTES: usize = 64 * 1024;

impl Hub {
    pub(super) fn attach(
        self: &Arc<Self>,
        client: u64,
        key: &str,
        cols: u16,
        rows: u16,
        id: Option<u64>,
    ) -> Result<(), (&'static str, String)> {
        let cols = cols.clamp(20, 1000);
        let rows = rows.clamp(5, 500);
        // A Codex thread the daemon does not list (a finished `codex exec`
        // of the team): Codex still knows it, so it is opened the same way.
        let unlisted = self.unlisted_codex_session(key);
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
            .or(unlisted)
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
    pub(super) fn spawn_terminal(
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
            state.ended_early.insert(
                terminal.to_string(),
                (exit_code, super::launch::plain_text(&tail)),
            );
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
}

pub(super) fn resize(t: &mut Terminal, cols: u16, rows: u16) {
    t.size = (cols, rows);
    let _ = t.master.resize(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    });
}
