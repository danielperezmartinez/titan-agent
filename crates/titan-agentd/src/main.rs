use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::thread;
use titan_agent_core::paths::{SOCKET_ENV, socket_path};
use titan_agentd::discover::{LiveSource, SourceConfig, SystemRunner};
use titan_agentd::server::{Daemon, DaemonConfig, DaemonError};
use titan_agentd::source::{FixtureSource, SessionSource};

fn main() {
    if let Err(code) = run() {
        std::process::exit(code)
    }
}
fn run() -> Result<(), i32> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut socket = None;
    let mut fixture = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].to_str() {
            Some("--socket") if i + 1 < args.len() => {
                socket = Some(PathBuf::from(&args[i + 1]));
                i += 2
            }
            Some("--fixture") if i + 1 < args.len() => {
                fixture = Some(PathBuf::from(&args[i + 1]));
                i += 2
            }
            _ => {
                eprintln!("usage: titan-agentd [--socket PATH] [--fixture FILE]");
                return Err(2);
            }
        }
    }
    let uid = std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or(0);
    let socket = socket.unwrap_or_else(|| {
        socket_path(
            std::env::var_os(SOCKET_ENV).as_deref(),
            std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
            uid,
        )
    });
    let source: Box<dyn SessionSource> = match fixture {
        Some(path) => match FixtureSource::from_file(&path) {
            Ok(source) => Box::new(source),
            Err(e) => {
                eprintln!("failed to load fixture: {e}");
                return Err(1);
            }
        },
        None => Box::new(LiveSource::new(
            SourceConfig::from_env(),
            Box::new(SystemRunner),
        )),
    };
    let daemon = match Daemon::bind(DaemonConfig::new(socket), source) {
        Ok(d) => d,
        Err(DaemonError::AlreadyRunning) => {
            eprintln!("titan-agentd is already running");
            return Ok(());
        }
        Err(DaemonError::Io(e)) => {
            eprintln!("titan-agentd: {e}");
            return Err(1);
        }
    };
    let shutdown = daemon.shutdown_handle();
    let signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGINT,
    ])
    .map_err(|e| {
        eprintln!("signal handler: {e}");
        1
    })?;
    let signal_shutdown = shutdown.clone();
    thread::spawn(move || {
        let mut signals = signals;
        let _ = signals.forever().next();
        signal_shutdown.shutdown();
    });
    daemon.run().map_err(|e| {
        eprintln!("titan-agentd: {e}");
        1
    })
}
