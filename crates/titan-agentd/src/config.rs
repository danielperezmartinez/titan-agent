use std::{env, os::unix::fs::MetadataExt, path::PathBuf, time::Duration};

#[derive(Clone, Debug)]
pub struct Config {
    pub socket: PathBuf,
    pub claude_program: String,
    pub codex_program: Option<String>,
    pub git_program: Option<String>,
    pub poll_subscribed: Duration,
    pub poll_idle: Duration,
}

impl Config {
    pub fn new(socket: PathBuf) -> Config {
        Config {
            socket,
            claude_program: "claude".into(),
            codex_program: Some("codex".into()),
            git_program: Some("git".into()),
            poll_subscribed: Duration::from_secs(2),
            poll_idle: Duration::from_secs(15),
        }
    }

    pub fn from_env_and_args(args: impl IntoIterator<Item = String>) -> Result<Config, String> {
        let mut config = Config::new(default_socket_path());
        if let Some(v) = env::var_os("TITAN_AGENTD_SOCKET") {
            config.socket = v.into();
        }
        if let Ok(v) = env::var("TITAN_AGENTD_CLAUDE") {
            config.claude_program = v;
        }
        if let Ok(v) = env::var("TITAN_AGENTD_CODEX") {
            config.codex_program = (!v.eq_ignore_ascii_case("off")).then_some(v);
        }
        if let Ok(v) = env::var("TITAN_AGENTD_GIT") {
            config.git_program = (!v.eq_ignore_ascii_case("off")).then_some(v);
        }
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--socket" => config.socket = args.next().ok_or("--socket requires a path")?.into(),
                _ => return Err(format!("unknown argument: {arg}")),
            }
        }
        Ok(config)
    }
}

pub fn default_socket_path() -> PathBuf {
    if let Some(runtime) = env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(runtime).join("titan-agent/agentd.sock");
    }
    let uid = std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or(0);
    env::temp_dir().join(format!("titan-agent-{uid}/agentd.sock"))
}
