use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub const SOCKET_ENV: &str = "TITAN_AGENT_SOCKET";

pub fn socket_path(
    override_path: Option<&OsStr>,
    xdg_runtime_dir: Option<&OsStr>,
    uid: u32,
) -> PathBuf {
    if let Some(path) = override_path.filter(|path| !path.is_empty()) {
        return PathBuf::from(path);
    }
    if let Some(runtime) = xdg_runtime_dir.filter(|path| !path.is_empty()) {
        return PathBuf::from(runtime)
            .join("titan-agent")
            .join("agentd.sock");
    }
    PathBuf::from(format!("/tmp/titan-agent-{uid}/agentd.sock"))
}

pub fn lock_path(socket: &Path) -> PathBuf {
    let mut value: OsString = socket.as_os_str().to_owned();
    value.push(".lock");
    PathBuf::from(value)
}
