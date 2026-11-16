//! titan-agentd: headless daemon of titan-agent (ADR-0004).
//!
//! Discovers Claude Code and Codex sessions, keeps the terminals attached to
//! them alive and serves the local protocol of ADR-0007 on a Unix socket.

mod claude;
mod codex;
mod hub;
mod quota;
mod team;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hub::{Config, Hub};
use titan_agent_core::protocol::socket_path;
use titan_agent_core::quota::Quota;
use titan_agent_core::session::{AgentCli, Session};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut explicit_socket = std::env::var("TITAN_AGENTD_SOCKET").ok();
    let mut fixture: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("titan-agentd {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--socket" => explicit_socket = args.next(),
            "--fixture" => fixture = args.next().map(PathBuf::from),
            "--print-socket" => {
                println!("{}", resolve_socket(explicit_socket.as_deref()).display());
                return;
            }
            "--help" | "-h" => {
                println!(
                    "titan-agentd {}\n\nUso: titan-agentd [--socket <ruta>] [--fixture <fichero>] [--print-socket] [--version]\n\n\
                     --fixture: sesiones y cuotas de prueba de un JSON ({{\"sessions\":[…],\"quotas\":[…]}})\n\
                     en lugar de las de los CLI; los eventos del equipo se leen igual.\n\n\
                     Variables: TITAN_AGENTD_SOCKET (ruta del socket), TITAN_AGENTD_POLL_MS\n\
                     (intervalo de sondeo), TITAN_AGENTD_CLAUDE y TITAN_AGENTD_CODEX (ejecutables).",
                    env!("CARGO_PKG_VERSION")
                );
                return;
            }
            other => {
                eprintln!("titan-agentd: argumento desconocido: {other}");
                std::process::exit(2);
            }
        }
    }

    let socket = resolve_socket(explicit_socket.as_deref());
    let config = Config {
        claude: std::env::var("TITAN_AGENTD_CLAUDE").unwrap_or_else(|_| "claude".into()),
        codex: std::env::var("TITAN_AGENTD_CODEX").unwrap_or_else(|_| "codex".into()),
        poll: Duration::from_millis(
            std::env::var("TITAN_AGENTD_POLL_MS")
                .ok()
                .and_then(|ms| ms.parse().ok())
                .unwrap_or(2000),
        ),
        home: std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from),
    };
    let hub = Hub::new(config);
    match fixture {
        Some(path) => spawn_fixture_poller(Arc::clone(&hub), path),
        None => {
            spawn_claude_poller(Arc::clone(&hub));
            spawn_codex_poller(Arc::clone(&hub));
            spawn_quota_poller(Arc::clone(&hub));
        }
    }
    spawn_team_poller(Arc::clone(&hub));
    if let Err(error) = server::serve(&socket, hub) {
        eprintln!("titan-agentd: {error}");
        std::process::exit(1);
    }
}

fn resolve_socket(explicit: Option<&str>) -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok();
    let temp = std::env::temp_dir();
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    PathBuf::from(socket_path(
        explicit,
        runtime.as_deref(),
        &temp.to_string_lossy(),
        &user,
    ))
}

fn spawn_claude_poller(hub: Arc<Hub>) {
    std::thread::spawn(move || {
        let mut source =
            claude::ClaudeSource::new(hub.config.claude.clone(), hub.config.home.clone());
        loop {
            hub.set_source(AgentCli::ClaudeCode, source.sessions());
            std::thread::sleep(hub.config.poll);
        }
    });
}

/// Test data instead of the CLIs: sessions and quotas from a JSON file,
/// read again on every poll so a script can change it.
fn spawn_fixture_poller(hub: Arc<Hub>, path: PathBuf) {
    std::thread::spawn(move || {
        loop {
            match read_fixture(&path) {
                Ok((sessions, quotas)) => {
                    let (claude, codex): (Vec<Session>, Vec<Session>) = sessions
                        .into_iter()
                        .partition(|s| s.agent == AgentCli::ClaudeCode);
                    hub.set_source(AgentCli::ClaudeCode, Ok(claude));
                    hub.set_source(AgentCli::Codex, Ok(codex));
                    hub.set_quotas(quotas);
                }
                Err(error) => {
                    hub.set_source(AgentCli::ClaudeCode, Err(error.clone()));
                    hub.set_source(AgentCli::Codex, Err(error));
                }
            }
            std::thread::sleep(hub.config.poll);
        }
    });
}

fn read_fixture(path: &std::path::Path) -> Result<(Vec<Session>, Vec<Quota>), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("no se pudo leer {}: {error}", path.display()))?;
    let bad = |error: serde_json::Error| format!("fixture {}: {error}", path.display());
    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(bad)?;
    let mut take = |key: &str| value.get_mut(key).map(serde_json::Value::take);
    let sessions = match take("sessions") {
        Some(list) => serde_json::from_value(list).map_err(bad)?,
        None => Vec::new(),
    };
    let quotas = match take("quotas") {
        Some(list) => serde_json::from_value(list).map_err(bad)?,
        None => Vec::new(),
    };
    Ok((sessions, quotas))
}

/// Reads the team tool's event logs next to the sessions every second.
fn spawn_team_poller(hub: Arc<Hub>) {
    std::thread::spawn(move || {
        let mut logs = team::TeamLogs::default();
        loop {
            let sessions = hub.sessions();
            hub.set_teams(logs.runs(&sessions, titan_agent_core::time::now_ms()));
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

/// Reads the CLIs' quotas every minute.
fn spawn_quota_poller(hub: Arc<Hub>) {
    std::thread::spawn(move || {
        loop {
            hub.set_quotas(quota::read(&hub.config.codex, hub.config.home.as_ref()));
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

fn spawn_codex_poller(hub: Arc<Hub>) {
    std::thread::spawn(move || {
        loop {
            let connected = codex::socket_path(&hub.config.codex)
                .and_then(|path| codex::CodexClient::connect(&path));
            match connected {
                Ok(mut client) => loop {
                    match client.sessions() {
                        Ok(sessions) => hub.set_source(AgentCli::Codex, Ok(sessions)),
                        Err(error) => {
                            hub.set_source(AgentCli::Codex, Err(error));
                            break;
                        }
                    }
                    std::thread::sleep(hub.config.poll);
                },
                Err(error) => hub.set_source(AgentCli::Codex, Err(error)),
            }
            std::thread::sleep(hub.config.poll.max(Duration::from_secs(5)));
        }
    });
}

#[cfg(unix)]
mod server {
    use std::fs::{File, OpenOptions};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::channel;

    use titan_agent_core::protocol::{ClientMessage, ServerMessage, decode_line, encode_line};

    use crate::hub::Hub;

    pub fn serve(socket: &Path, hub: Arc<Hub>) -> Result<(), String> {
        if let Some(dir) = socket.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|error| format!("no se pudo crear {}: {error}", dir.display()))?;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
        // One daemon per socket: hold an exclusive lock for our whole life.
        let lock_path = socket.with_extension("sock.lock");
        let lock: File = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|error| format!("no se pudo abrir {}: {error}", lock_path.display()))?;
        if lock.try_lock().is_err() {
            return Err(format!("ya hay un titan-agentd en {}", socket.display()));
        }
        let _ = std::fs::remove_file(socket);
        let listener = UnixListener::bind(socket)
            .map_err(|error| format!("no se pudo escuchar en {}: {error}", socket.display()))?;
        let _ = std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600));
        eprintln!(
            "titan-agentd {} escuchando en {}",
            env!("CARGO_PKG_VERSION"),
            socket.display()
        );

        let next_client = AtomicU64::new(1);
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let id = next_client.fetch_add(1, Ordering::Relaxed);
            let hub = Arc::clone(&hub);
            std::thread::spawn(move || client(id, stream, hub));
        }
        drop(lock);
        Ok(())
    }

    fn client(id: u64, stream: UnixStream, hub: Arc<Hub>) {
        let Ok(mut writer) = stream.try_clone() else {
            return;
        };
        let (tx, rx) = channel::<String>();
        hub.add_client(id, tx.clone());
        std::thread::spawn(move || {
            for line in rx {
                if writer.write_all(line.as_bytes()).is_err() {
                    break;
                }
            }
            let _ = writer.shutdown(std::net::Shutdown::Both);
        });
        let mut hello_done = false;
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match decode_line::<ClientMessage>(&line) {
                Ok(message) => hub.handle(id, message, &mut hello_done),
                Err(error) => {
                    // Answer malformed requests with their id when it can be
                    // recovered, and with a null id otherwise.
                    let request_id = serde_json::from_str::<serde_json::Value>(&line)
                        .ok()
                        .and_then(|v| v.get("id").and_then(serde_json::Value::as_u64));
                    let answer = match request_id {
                        Some(request_id) => {
                            ServerMessage::error(request_id, "bad_request", error.to_string())
                        }
                        None => ServerMessage::bad_line(error.to_string()),
                    };
                    let _ = tx.send(encode_line(&answer));
                }
            }
        }
        hub.remove_client(id);
    }
}

#[cfg(not(unix))]
mod server {
    use std::path::Path;
    use std::sync::Arc;

    use crate::hub::Hub;

    pub fn serve(_socket: &Path, _hub: Arc<Hub>) -> Result<(), String> {
        Err("el socket local de Windows (named pipe) aún no está implementado".into())
    }
}
