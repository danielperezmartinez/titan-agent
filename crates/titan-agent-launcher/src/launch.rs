//! Starting the installed interface (ADR-0011 §7): count the attempts of a
//! version just applied and roll back one that failed to start twice.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use titan_agent_core::protocol::{PID_FILE, socket_name, socket_path_named};
use titan_agent_core::update::{LaunchStep, Pending, RolledBack, launch_step};

/// Runs `current/bin/titan-agent` with `args`, after the step that a
/// pending version asks for. Only returns on error.
pub fn launch(args: Vec<String>) -> Result<(), String> {
    let root = crate::install_dir()?;
    let update = root.join("update");
    let pending_path = update.join("pending.json");
    let pending: Option<Pending> = std::fs::read(&pending_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    match launch_step(pending.as_ref()) {
        LaunchStep::Run => {}
        LaunchStep::Count(next) => write_json(&pending_path, &next)?,
        LaunchStep::RollBack { from, to } => roll_back(&root, &from, &to)?,
    }
    let ui = root.join("current/bin/titan-agent");
    if !ui.is_file() {
        return Err(format!(
            "titan-agent no está instalado ({} no existe); instálalo con install.sh",
            ui.display()
        ));
    }
    let error = Command::new(&ui)
        .args(args)
        .env("TITAN_AGENT_LAUNCHER", root.join("titan-agent-launcher"))
        .exec();
    Err(format!("no se pudo ejecutar {}: {error}", ui.display()))
}

/// Goes back from `to` (failed to start twice) to `from`.
fn roll_back(root: &Path, from: &str, to: &str) -> Result<(), String> {
    let update = root.join("update");
    if !root.join("versions").join(from).is_dir() {
        return Err(format!(
            "la versión {to} no arrancó y la {from} ya no está instalada: vuelve a instalar"
        ));
    }
    point_current(root, from)?;
    let bad_path = update.join("bad.json");
    let mut bad: Vec<String> = std::fs::read(&bad_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    if !bad.iter().any(|v| v == to) {
        bad.push(to.to_string());
    }
    write_json(&bad_path, &bad)?;
    stop_daemon_of(root, to);
    let _ = std::fs::remove_file(update.join("pending.json"));
    write_json(
        &update.join("rolled-back.json"),
        &RolledBack {
            from: from.to_string(),
            to: to.to_string(),
        },
    )?;
    // The only system notification of the app (the visual decision allows
    // it here: there is no window yet). Best effort.
    let _ = Command::new("notify-send")
        .arg("titan-agent")
        .arg(format!(
            "La versión {to} no arrancó; se ha vuelto a la {from}."
        ))
        .status();
    Ok(())
}

/// Points `root/current` at `versions/<version>`, replacing the link at
/// once with a rename.
pub fn point_current(root: &Path, version: &str) -> Result<(), String> {
    let temp = root.join(format!(".current-{}", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    std::os::unix::fs::symlink(Path::new("versions").join(version), &temp)
        .and_then(|()| std::fs::rename(&temp, root.join("current")))
        .map_err(|error| format!("no se pudo cambiar current: {error}"))
}

/// Ends the daemon of the bad version, found by the pid it writes next to
/// its socket, only if that process runs from `versions/<to>/`.
fn stop_daemon_of(root: &Path, to: &str) {
    let Some(pid) = daemon_pid() else { return };
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).unwrap_or_default();
    let bad_dir = root.join("versions").join(to);
    let bad_dir = bad_dir.canonicalize().unwrap_or(bad_dir);
    if !exe.starts_with(&bad_dir) {
        return;
    }
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
}

fn daemon_pid() -> Option<u32> {
    let socket = PathBuf::from(socket_path_named(
        std::env::var("TITAN_AGENTD_SOCKET").ok().as_deref(),
        std::env::var("XDG_RUNTIME_DIR").ok().as_deref(),
        &std::env::temp_dir().to_string_lossy(),
        &std::env::var("USER").unwrap_or_default(),
        socket_name(cfg!(debug_assertions)),
    ));
    let text = std::fs::read_to_string(socket.parent()?.join(PID_FILE)).ok()?;
    text.trim().parse().ok().filter(|pid| *pid > 1)
}

/// Writes `value` as JSON through a temporary file and a rename.
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|error| error.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temp, text)
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|error| format!("no se pudo escribir {}: {error}", path.display()))
}
