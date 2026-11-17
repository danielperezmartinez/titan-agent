//! `install` and `uninstall` (ADR-0011 §4): one folder per version, the
//! `current` link, a stable launcher, the links in `~/.local/bin` and the
//! menu entry. Installing is idempotent and never touches the settings.

use std::path::{Path, PathBuf};
use std::process::Command;

use titan_agent_core::update::{APP_ID, BINARIES, Version};

use crate::launch::point_current;

/// The folder this launcher was unpacked in: the parent of its `bin/`.
fn bundle_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let bin = exe.parent().filter(|dir| dir.file_name().is_some_and(|n| n == "bin"));
    bin.and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("{} no está dentro de una carpeta bin/", exe.display()))
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "falta HOME".to_string())
}

/// Folder of the `.desktop` and the icon: the data folder.
fn data_dir(root: &Path) -> &Path {
    root.parent().unwrap_or(root)
}

fn desktop_path(root: &Path) -> PathBuf {
    data_dir(root)
        .join("applications")
        .join(format!("{APP_ID}.desktop"))
}

fn icon_path(root: &Path) -> PathBuf {
    data_dir(root)
        .join("icons/hicolor/scalable/apps")
        .join(format!("{APP_ID}.svg"))
}

pub fn install() -> Result<(), String> {
    let bundle = bundle_dir()?;
    for binary in BINARIES {
        if !bundle.join("bin").join(binary).is_file() {
            return Err(format!(
                "falta bin/{binary} en {}: el paquete está incompleto",
                bundle.display()
            ));
        }
    }
    let version = env!("CARGO_PKG_VERSION");
    let root = crate::install_dir()?;
    let versions = root.join("versions");
    std::fs::create_dir_all(&versions).map_err(|e| format!("{}: {e}", versions.display()))?;
    let target = versions.join(version);
    if !target.exists() {
        let temp = versions.join(format!(".install-{version}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        copy_tree(&bundle, &temp)
            .and_then(|()| std::fs::rename(&temp, &target))
            .map_err(|error| {
                let _ = std::fs::remove_dir_all(&temp);
                format!("no se pudo copiar a {}: {error}", target.display())
            })?;
    }
    point_current(&root, version)?;
    update_launcher(&root, &target.join("bin/titan-agent-launcher"))?;

    let home = home()?;
    let bin = home.join(".local/bin");
    link(&root.join("titan-agent-launcher"), &bin.join("titan-agent"))?;
    link(&root.join("current/bin/titan-team"), &bin.join("titan-team"))?;

    let icon_source = target.join("share").join(format!("{APP_ID}.svg"));
    let icon = if icon_source.is_file() {
        let path = icon_path(&root);
        copy_file(&icon_source, &path)?;
        APP_ID.to_string()
    } else {
        // Until the user approves an icon of its own.
        "utilities-terminal".to_string()
    };
    let desktop = desktop_entry(&bin.join("titan-agent"), &icon);
    let path = desktop_path(&root);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(desktop.as_str()) {
        write_file(&path, desktop.as_bytes())?;
    }
    println!(
        "titan-agent {version} instalado en {}; ábrelo desde el menú o con {}",
        root.display(),
        bin.join("titan-agent").display()
    );
    Ok(())
}

fn desktop_entry(exec: &Path, icon: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=titan-agent\n\
         GenericName=Torre de control de agentes\n\
         Comment=Sesiones de Claude Code y Codex en una sola ventana\n\
         Exec={}\nIcon={icon}\nTerminal=false\nCategories=Development;\n\
         StartupWMClass={APP_ID}\n",
        exec.display()
    )
}

/// Copies the launcher of the installed version to the stable path when
/// there is none or the one there is older.
fn update_launcher(root: &Path, source: &Path) -> Result<(), String> {
    let stable = root.join("titan-agent-launcher");
    let ours = Version::parse(env!("CARGO_PKG_VERSION"));
    let theirs = launcher_version(&stable);
    if stable.is_file() && theirs.is_some_and(|theirs| Some(theirs) >= ours) {
        return Ok(());
    }
    copy_file(source, &stable)
}

/// Version of an installed launcher, from its `--version`.
pub fn launcher_version(path: &Path) -> Option<Version> {
    let output = Command::new(path).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    Version::parse(text.trim().strip_prefix("titan-agent-launcher ")?)
}

/// Replaces `link` with a symbolic link to `target` (absolute).
fn link(target: &Path, link: &Path) -> Result<(), String> {
    if std::fs::read_link(link).ok().as_deref() == Some(target) {
        return Ok(());
    }
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let temp = link.with_extension(format!("tmp-{}", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    std::os::unix::fs::symlink(target, &temp)
        .and_then(|()| std::fs::rename(&temp, link))
        .map_err(|error| format!("no se pudo crear {}: {error}", link.display()))
}

/// Copies one file through a temporary one and a rename.
fn copy_file(source: &Path, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::copy(source, &temp)
        .and_then(|_| std::fs::rename(&temp, path))
        .map_err(|error| format!("no se pudo copiar {}: {error}", path.display()))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temp, bytes)
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|error| format!("no se pudo escribir {}: {error}", path.display()))
}

/// Copies a folder with its permissions and symbolic links.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_symlink() {
            std::os::unix::fs::symlink(std::fs::read_link(entry.path())?, &target)?;
        } else if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    let root = crate::install_dir()?;
    let bin = home()?.join(".local/bin");
    for name in ["titan-agent", "titan-team"] {
        let path = bin.join(name);
        if std::fs::read_link(&path).is_ok_and(|target| target.starts_with(&root)) {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    for path in [desktop_path(&root), icon_path(&root)] {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    println!("titan-agent desinstalado (la configuración se conserva)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_entry_runs_the_stable_link_with_the_wayland_id() {
        let entry = desktop_entry(Path::new("/h/.local/bin/titan-agent"), "utilities-terminal");
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("\nExec=/h/.local/bin/titan-agent\n"));
        assert!(entry.contains("\nIcon=utilities-terminal\n"));
        assert!(entry.ends_with(&format!("StartupWMClass={APP_ID}\n")));
    }
}
