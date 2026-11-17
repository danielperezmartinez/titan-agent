//! Applying a prepared version (ADR-0011 §7): `pending.json`, `current`
//! switched with a rename, the stable launcher updated, and `exec` of the
//! new daemon on the same socket.

use std::path::{Path, PathBuf};

use titan_agent_core::update::{Pending, Version};

/// Writes `pending` (a new version), points `current` at `to` and updates
/// the launcher, all or nothing: the new launcher is copied aside first,
/// and any failure after a change undoes it. Returns the daemon to run.
pub fn switch(root: &Path, to: Version, pending: Option<&Pending>) -> Result<PathBuf, String> {
    let dir = root.join("versions").join(to.to_string());
    let daemon = dir.join("bin/titan-agentd");
    if !daemon.is_file() {
        return Err(format!("falta {}", daemon.display()));
    }
    let previous = std::fs::read_link(root.join("current")).ok();
    let stable = root.join("titan-agent-launcher");
    let launcher = stable.with_extension(format!("tmp-{}", std::process::id()));
    if pending.is_some() {
        std::fs::copy(dir.join("bin/titan-agent-launcher"), &launcher)
            .map_err(|e| format!("no se pudo copiar el lanzador nuevo: {e}"))?;
    }
    let pending_path = root.join("update/pending.json");
    let undo = |error: String| {
        let _ = std::fs::remove_file(&launcher);
        if pending.is_some() {
            let _ = std::fs::remove_file(&pending_path);
        }
        if let Some(previous) = previous.as_ref().and_then(|p| p.file_name()) {
            let _ = point_current(root, &previous.to_string_lossy());
        }
        error
    };
    if let Some(pending) = pending {
        let text = serde_json::to_string(pending).map_err(|e| undo(e.to_string()))?;
        write(&pending_path, text.as_bytes()).map_err(undo)?;
    }
    point_current(root, &to.to_string()).map_err(undo)?;
    if pending.is_some() {
        std::fs::rename(&launcher, &stable)
            .map_err(|e| undo(format!("no se pudo actualizar el lanzador: {e}")))?;
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn layout(name: &str, with_launcher: bool) -> PathBuf {
        let root = std::env::temp_dir().join(format!("titan-apply-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for v in ["0.6.0", "0.6.1"] {
            let bin = root.join("versions").join(v).join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("titan-agentd"), v).unwrap();
            if with_launcher || v == "0.6.0" {
                std::fs::write(bin.join("titan-agent-launcher"), v).unwrap();
            }
        }
        std::fs::write(root.join("titan-agent-launcher"), "0.6.0").unwrap();
        point_current(&root, "0.6.0").unwrap();
        root
    }

    fn pending() -> Pending {
        Pending {
            from: "0.6.0".into(),
            to: "0.6.1".into(),
            attempts: 0,
        }
    }

    #[test]
    fn switching_writes_pending_current_and_the_launcher() {
        let root = layout("ok", true);
        let to = Version::parse("0.6.1").unwrap();
        let daemon = switch(&root, to, Some(&pending())).unwrap();
        assert_eq!(daemon, root.join("versions/0.6.1/bin/titan-agentd"));
        assert_eq!(
            std::fs::read_link(root.join("current")).unwrap(),
            Path::new("versions/0.6.1")
        );
        assert!(root.join("update/pending.json").is_file());
        assert_eq!(
            std::fs::read_to_string(root.join("titan-agent-launcher")).unwrap(),
            "0.6.1"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_failed_switch_changes_nothing() {
        let root = layout("fail", false);
        let to = Version::parse("0.6.1").unwrap();
        assert!(switch(&root, to, Some(&pending())).is_err());
        assert_eq!(
            std::fs::read_link(root.join("current")).unwrap(),
            Path::new("versions/0.6.0")
        );
        assert!(!root.join("update/pending.json").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("titan-agent-launcher")).unwrap(),
            "0.6.0"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
