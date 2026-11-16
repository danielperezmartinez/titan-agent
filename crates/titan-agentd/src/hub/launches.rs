//! Sessions launched from «Nueva sesión» (ADR-0002): the safeguards checked
//! before running the CLI, the single run of `claude --bg` or the Codex TUI,
//! and how each launch is matched to the real session the CLI reports.

use super::*;

/// The same agent, folder and brief cannot be launched again within this
/// time (a double click), whatever the token says.
const REPEAT_WINDOW: Duration = Duration::from_secs(30);
/// Longest wait for `claude --bg` to return.
const CLAUDE_LAUNCH_TIMEOUT: Duration = Duration::from_secs(60);
/// How long the provisional session of a Claude Code launch stays in the
/// tree if the CLI never reports the real one.
const PROVISIONAL_TTL: Duration = Duration::from_secs(120);
/// How long a launched TUI must stay up before its launch counts as done.
const EARLY_EXIT_WINDOW: Duration = Duration::from_millis(1000);
/// Terminal size of a launched TUI until the interface resizes it.
const LAUNCH_SIZE: (u16, u16) = (120, 40);

/// A session launched by this daemon (ADR-0002).
pub(super) struct Launched {
    /// Provisional key, `launch:<n>`.
    pub(super) key: String,
    spec: LaunchSpec,
    at_ms: i64,
    at: Instant,
    /// The CLI is still being run (`claude --bg` has not returned).
    pub(super) running: bool,
    /// Id printed by `claude --bg`.
    cli_id: Option<String>,
    /// Keys of the sessions known when it was launched.
    known: HashSet<String>,
    /// The real session, once discovered.
    pub(super) session: Option<String>,
}

impl Hub {
    /// Handles a `launch` request: checks every safeguard of ADR-0002, then
    /// runs the official CLI once. Answers the client itself.
    pub(super) fn launch(
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
                let terminal = match spawned {
                    Ok(terminal) => terminal,
                    Err(error) => {
                        finish_launch(&mut state, &key, None);
                        if let Some(id) = id {
                            send_to(&state, client, launch_answer(id, Err(error)));
                        }
                        return;
                    }
                };
                drop(state);
                // A TUI that ends right away (bad option, no login…) is a
                // failed launch, with what it printed as the reason.
                let hub = Arc::clone(self);
                std::thread::spawn(move || {
                    let start = Instant::now();
                    let early = loop {
                        let ended = hub.lock().ended_early.remove(&terminal);
                        if ended.is_some() || start.elapsed() >= EARLY_EXIT_WINDOW {
                            break ended;
                        }
                        std::thread::sleep(Duration::from_millis(40));
                    };
                    let answer = match early {
                        None => Ok(LaunchResult {
                            launch: key.clone(),
                            agent: spec.agent,
                            terminal: Some(terminal),
                            cli_id: None,
                        }),
                        Some((code, output)) => Err((
                            "launch_failed",
                            failure_message(
                                &output,
                                &code.map_or("un error".into(), |c| format!("el código {c}")),
                            ),
                        )),
                    };
                    let mut state = hub.lock();
                    finish_launch(&mut state, &key, answer.as_ref().ok().map(|_| None));
                    if let Some(id) = id {
                        send_to(&state, client, launch_answer(id, answer));
                    }
                });
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
}

/// Links each launch to the real session once the CLI reports it, moves its
/// terminal there, and shows a provisional session until then.
pub(super) fn resolve_launches(state: &mut State, sessions: &mut Vec<Session>) {
    let State {
        launches,
        terminals,
        ..
    } = state;
    let mut claimed: HashSet<String> = launches.iter().filter_map(|l| l.session.clone()).collect();
    let pending_codex: Vec<(String, String)> = launches
        .iter()
        .filter(|l| {
            l.spec.agent == AgentCli::Codex
                && l.session.is_none()
                && terminals.values().any(|t| t.session == l.key)
        })
        .map(|l| (l.key.clone(), l.spec.cwd.clone()))
        .collect();
    let mut provisional = Vec::new();
    for launch in launches.iter_mut() {
        if launch.session.is_none() {
            let found = match launch.spec.agent {
                AgentCli::ClaudeCode => launch.cli_id.as_ref().and_then(|cli_id| {
                    sessions
                        .iter()
                        .find(|s| s.agent == AgentCli::ClaudeCode && &s.cli_id == cli_id)
                }),
                AgentCli::Codex => {
                    // Only while its TUI is open, and only when the match is
                    // unambiguous: one new thread and one pending launch in
                    // that folder.
                    let alive = terminals.values().any(|t| t.session == launch.key);
                    let candidates: Vec<&Session> = sessions
                        .iter()
                        .filter(|s| {
                            !launch.known.contains(&s.key)
                                && !claimed.contains(&s.key)
                                && launch.spec.matches_new_session(s, launch.at_ms)
                        })
                        .collect();
                    let rivals = pending_codex
                        .iter()
                        .filter(|(key, cwd)| {
                            *key != launch.key && same_folder(cwd, &launch.spec.cwd)
                        })
                        .count();
                    (alive && candidates.len() == 1 && rivals == 0).then(|| candidates[0])
                }
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
    // «lanzada por: tú (titan-agent)» for every session launched here.
    for launch in launches.iter() {
        if let Some(real) = &launch.session
            && let Some(session) = sessions.iter_mut().find(|s| &s.key == real)
        {
            session.launched_here = true;
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
    let folder = folder_identity(&spec.cwd);
    let same = |other: &LaunchSpec| {
        other.agent == spec.agent
            && other.brief == spec.brief
            && folder_identity(&other.cwd) == folder
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

/// The same folder for de-duplication, whatever its spelling
/// (`/a/b/`, `/a/./b`, a link to it).
fn folder_identity(cwd: &str) -> PathBuf {
    std::fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd.trim_end_matches('/')))
}

fn same_folder(a: &str, b: &str) -> bool {
    folder_identity(a) == folder_identity(b)
}

/// Terminal output without escape sequences, for an error message.
pub(super) fn plain_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut plain = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip `ESC [ … letter`, `ESC ] … BEL` and two-char escapes.
            match chars.next() {
                Some('[') => {
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() || c == '~' {
                            break;
                        }
                    }
                }
                Some(']') => {
                    for c in chars.by_ref() {
                        if c == '\u{7}' {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if c == '\n' || !c.is_control() {
            plain.push(c);
        }
    }
    plain
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
    // Created exclusively (never through an existing file or link) and
    // readable only by this user.
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut out = options.open(&out_path).map_err(|e| failed(e.to_string()))?;
    let stdout = out.try_clone().map_err(|e| failed(e.to_string()))?;
    let err = out.try_clone().map_err(|e| failed(e.to_string()))?;
    let spawned = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(stdout)
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
        let mut output = String::new();
        let _ = std::io::Seek::seek(&mut out, std::io::SeekFrom::Start(0));
        let _ = std::io::Read::read_to_string(&mut out, &mut output);
        if status.success() {
            Ok(output)
        } else {
            Err(failed(failure_message(&output, &status.to_string())))
        }
    })();
    let _ = std::fs::remove_file(&out_path);
    result
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

    /// A real PTY (running `sleep`) bound to `session`.
    fn fake_terminal(session: &str) -> Terminal {
        let pair = native_pty_system()
            .openpty(PtySize::default())
            .expect("a pty for the test");
        let mut sleep = CommandBuilder::new("sleep");
        sleep.arg("30");
        let child = pair.slave.spawn_command(sleep).expect("spawn sleep");
        Terminal {
            session: session.into(),
            killer: child.clone_killer(),
            master: pair.master,
            input: channel().0,
            history: VecDeque::new(),
            viewers: HashSet::new(),
            size: (80, 24),
        }
    }

    fn codex_launch(key: &str, cwd: &str, known: &[&str]) -> Launched {
        Launched {
            key: key.into(),
            spec: LaunchSpec {
                agent: AgentCli::Codex,
                cwd: cwd.into(),
                model: "gpt-6-luna".into(),
                effort: "low".into(),
                permission: "read-only".into(),
                brief: key.into(),
            },
            at_ms: now_ms(),
            at: Instant::now(),
            running: false,
            cli_id: None,
            known: known.iter().map(|k| k.to_string()).collect(),
            session: None,
        }
    }

    fn codex_thread(key: &str, cwd: &str, started_at_ms: i64) -> Session {
        let spec = LaunchSpec {
            agent: AgentCli::Codex,
            cwd: cwd.into(),
            model: "gpt-6-luna".into(),
            effort: "low".into(),
            permission: "read-only".into(),
            brief: "x".into(),
        };
        let mut thread = spec.provisional_session(key, started_at_ms);
        thread.launched_here = false;
        thread
    }

    #[test]
    fn folder_identity_ignores_spelling() {
        let dir = std::env::temp_dir();
        let plain = dir.to_string_lossy().trim_end_matches('/').to_string();
        assert_eq!(
            folder_identity(&plain),
            folder_identity(&format!("{plain}/"))
        );
        assert_eq!(
            folder_identity(&plain),
            folder_identity(&format!("{plain}/./"))
        );
        assert_eq!(
            plain_text(b"\x1b[31merror:\x1b[0m bad\r\n\x1b]0;title\x07x"),
            "error: bad\nx"
        );
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
        {
            let mut state = hub.lock();
            state
                .launches
                .push(codex_launch("launch:1", "/srv/demo/kite", &["cx:old"]));
            state
                .terminals
                .insert("t1".into(), fake_terminal("launch:1"));
            rebuild(&mut state);
            assert!(state.sessions.iter().any(|s| s.key == "launch:1"));
        }
        // A thread in another folder and the old one do not match.
        let other = codex_thread("cx:other", "/srv/demo/else", now_ms());
        let new = codex_thread("cx:new", "/srv/demo/kite/", now_ms());
        hub.set_source(AgentCli::Codex, Ok(vec![old, other, new]));
        let mut state = hub.lock();
        assert_eq!(state.launches[0].session.as_deref(), Some("cx:new"));
        assert!(!state.sessions.iter().any(|s| s.key == "launch:1"));
        assert_eq!(state.terminals["t1"].session, "cx:new");
        let _ = state.terminals.get_mut("t1").map(|t| t.killer.kill());
    }

    #[test]
    fn codex_launch_without_its_tui_or_with_rivals_stays_unresolved() {
        let hub = hub();
        {
            let mut state = hub.lock();
            // launch:1 closed its TUI; launch:2 and launch:3 share a folder.
            state
                .launches
                .push(codex_launch("launch:1", "/srv/demo/a", &[]));
            state
                .launches
                .push(codex_launch("launch:2", "/srv/demo/b", &[]));
            state
                .launches
                .push(codex_launch("launch:3", "/srv/demo/b", &[]));
            state
                .terminals
                .insert("t2".into(), fake_terminal("launch:2"));
            state
                .terminals
                .insert("t3".into(), fake_terminal("launch:3"));
        }
        hub.set_source(
            AgentCli::Codex,
            Ok(vec![
                codex_thread("cx:a", "/srv/demo/a", now_ms()),
                codex_thread("cx:b", "/srv/demo/b", now_ms()),
            ]),
        );
        let mut state = hub.lock();
        assert!(state.launches.iter().all(|l| l.session.is_none()));
        for t in state.terminals.values_mut() {
            let _ = t.killer.kill();
        }
    }

    #[test]
    fn a_resolved_launch_marks_its_session_as_launched_here() {
        let hub = hub();
        let mut launch = codex_launch("launch:1", "/srv/demo/kite", &[]);
        launch.session = Some("cx:new".into());
        hub.lock().launches.push(launch);
        hub.set_source(
            AgentCli::Codex,
            Ok(vec![
                codex_thread("cx:new", "/srv/demo/kite", now_ms()),
                codex_thread("cx:other", "/srv/demo/kite", now_ms()),
            ]),
        );
        let state = hub.lock();
        let marked = |key: &str| {
            state
                .sessions
                .iter()
                .find(|s| s.key == key)
                .map(|s| s.launched_here)
        };
        assert_eq!(marked("cx:new"), Some(true));
        assert_eq!(marked("cx:other"), Some(false));
    }
}
