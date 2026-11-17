//! titan-agent-launcher: installs titan-agent for the user, starts the
//! current version and rolls back a version that does not start
//! (docs/Decisiones/ADR-0011, §4 and §7).
//!
//! Without arguments (what the menu entry and `~/.local/bin/titan-agent`
//! run) it starts `current/bin/titan-agent`; `install` and `uninstall`
//! manage the install under `$XDG_DATA_HOME/titan-agent`. Only Linux is
//! installed this way; on Windows the zip is portable (§5).

#[cfg(unix)]
mod install;
#[cfg(unix)]
mod launch;

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    let code = match command.as_deref() {
        Some("--version" | "-V") => {
            println!("titan-agent-launcher {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("--help" | "-h") => {
            println!(
                "titan-agent-launcher {}\n\nUso:\n  titan-agent-launcher [argumentos de titan-agent]\n  \
                 titan-agent-launcher install\n  titan-agent-launcher uninstall\n  titan-agent-launcher --version",
                env!("CARGO_PKG_VERSION")
            );
            0
        }
        #[cfg(unix)]
        Some("install") => report(install::install()),
        #[cfg(unix)]
        Some("uninstall") => report(install::uninstall()),
        #[cfg(unix)]
        None => report(launch::launch(Vec::new())),
        #[cfg(unix)]
        Some(arg) if arg.starts_with('-') => {
            report(launch::launch(std::iter::once(arg.to_string()).chain(args).collect()))
        }
        #[cfg(not(unix))]
        None | Some("install" | "uninstall") => {
            eprintln!("titan-agent-launcher: en Windows la versión es portable (sin instalar)");
            1
        }
        Some(other) => {
            eprintln!("titan-agent-launcher: orden desconocida: {other} (--help)");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(unix)]
fn report(result: Result<(), String>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("titan-agent-launcher: {error}");
            1
        }
    }
}

/// Where titan-agent is installed: `<data>/titan-agent`.
pub fn install_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").ok();
    let xdg = std::env::var("XDG_DATA_HOME").ok();
    titan_agent_core::update::data_home(home.as_deref(), xdg.as_deref())
        .map(|data| data.join(titan_agent_core::update::INSTALL_DIR))
        .ok_or_else(|| "no se sabe dónde instalar: falta HOME".to_string())
}
