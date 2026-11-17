//! Applying a prepared version (ADR-0011 §7): `pending.json`, `current`
//! switched with a rename, the stable launcher updated, and `exec` of the
//! new daemon on the same socket.

use std::path::{Path, PathBuf};

use titan_agent_core::update::{Pending, Version};

/// Writes `pending` (a new version), points `current` at `to` and updates
/// the launcher. Returns the daemon to run.
pub fn switch(root: &Path, to: Version, pending: Option<&Pending>) -> Result<PathBuf, String> {
    let dir = root.join("versions").join(to.to_string());
    let daemon = dir.join("bin/titan-agentd");
    if !daemon.is_file() {
        return Err(format!("falta {}", daemon.display()));
    }
    if let Some(pending) = pending {
        let text = serde_json::to_string(pending).map_err(|e| e.to_string())?;
        write(&root.join("update/pending.json"), text.as_bytes())?;
    }
    point_current(root, &to.to_string())?;
    if pending.is_some() {
        let source = dir.join("bin/titan-agent-launcher");
        let stable = root.join("titan-agent-launcher");
        let temp = stable.with_extension(format!("tmp-{}", std::process::id()));
        std::fs::copy(&source, &temp)
            .and_then(|_| std::fs::rename(&temp, &stable))
            .map_err(|e| format!("no se pudo actualizar el lanzador: {e}"))?;
    }
    Ok(daemon)
}

/// Undoes [`switch`] after `exec` failed: back to `running`.
pub fn undo(root: &Path, running: Version, had_pending: bool) {
    if had_pending {
        let _ = std::fs::remove_file(root.join("update/pending.json"));
    }
    let _ = point_current(root, &running.to_string());
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temp, bytes)
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|e| format!("no se pudo escribir {}: {e}", path.display()))
}

#[cfg(unix)]
fn point_current(root: &Path, version: &str) -> Result<(), String> {
    let temp = root.join(format!(".current-{}", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    std::os::unix::fs::symlink(Path::new("versions").join(version), &temp)
        .and_then(|()| std::fs::rename(&temp, root.join("current")))
        .map_err(|e| format!("no se pudo cambiar current: {e}"))
}

#[cfg(not(unix))]
fn point_current(_root: &Path, _version: &str) -> Result<(), String> {
    Err("el actualizador aún no funciona en Windows".into())
}

/// Replaces this process with `daemon`, with the same arguments and
/// environment. Returns only on failure. The lock, the listening socket and
/// the terminals' PTYs are close-on-exec, so the new daemon takes them over
/// from scratch.
#[cfg(unix)]
pub fn exec(daemon: &Path) -> String {
    use std::os::unix::process::CommandExt;
    let error = std::process::Command::new(daemon)
        .args(std::env::args_os().skip(1))
        .exec();
    format!("no se pudo ejecutar {}: {error}", daemon.display())
}

#[cfg(not(unix))]
pub fn exec(_daemon: &Path) -> String {
    "el actualizador aún no funciona en Windows".into()
}
