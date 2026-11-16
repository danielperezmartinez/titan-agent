//! The interface's own settings file: the defaults of the "new session" box,
//! changed from `[*]`. It never reads any file of the agent CLIs.

use std::path::PathBuf;

use titan_agent_core::launch::LaunchDefaults;

/// `$XDG_CONFIG_HOME/titan-agent/defaults.json`, or `~/.config/…`.
/// `TITAN_AGENT_CONFIG_DIR` overrides the folder (tests and screenshots).
fn path() -> Option<PathBuf> {
    let dir = std::env::var_os("TITAN_AGENT_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .filter(|dir| !dir.is_empty())
                .map(|dir| PathBuf::from(dir).join("titan-agent"))
        })
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/titan-agent"))
        })?;
    Some(dir.join("defaults.json"))
}

pub fn load() -> LaunchDefaults {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| LaunchDefaults::parse(&text))
        .unwrap_or_default()
}

pub fn save(defaults: &LaunchDefaults) -> Result<(), String> {
    let path = path().ok_or("no hay carpeta de configuración")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(defaults).map_err(|error| error.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|error| error.to_string())
}
