//! The updater of an installed titan-agent (docs/Decisiones/ADR-0011, §6
//! and §7). It checks 30 s after start and every 6 h a manifest signed with
//! minisign, downloads and verifies the new version in the background,
//! extracts it to `versions/<v>` and applies it when it is safe: no window
//! connected and no working session with a terminal here. Applying points
//! `current` at it and replaces this process with `exec`.
//!
//! It is only active when this executable runs from
//! `…/titan-agent/versions/<v>/bin/` and `TITAN_AGENT_NO_UPDATE` is unset:
//! a build tree never checks nor applies anything. The pure decisions live
//! in `titan_agent_core::update`; downloading is in `update/fetch.rs`,
//! preparing a version in `update/prepare.rs` and applying in
//! `update/apply.rs`.

mod apply;
mod fetch;
mod prepare;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use titan_agent_core::protocol::{UpdateState, UpdateStatus};
use titan_agent_core::update::{
    LINUX_TARGET, Pending, RELEASES_URL, UpdateError, Version, accepted_keys,
    config_dir, install_root, select, verify_manifest, versions_to_remove,
};

use crate::hub::{Hub, now_ms};

/// The first check, after the daemon starts.
const FIRST_CHECK: Duration = Duration::from_secs(30);
/// Between automatic checks.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Where the running daemon is installed.
#[derive(Debug, Clone)]
pub struct Install {
    pub root: PathBuf,
    pub running: Version,
}

pub struct Updater {
    install: Option<Install>,
    config_dir: Option<PathBuf>,
    base_url: String,
    keys: Vec<String>,
    first_check: Duration,
    inner: Mutex<Inner>,
    wake: Mutex<Option<Sender<()>>>,
}

#[derive(Default)]
struct Inner {
    status: UpdateStatus,
    /// The version extracted to `versions/<v>`, ready to apply.
    prepared: Option<Version>,
}

impl Updater {
    /// An updater that never does anything (tests of the hub).
    #[cfg(test)]
    pub fn inactive() -> Updater {
        Updater::new(None, None)
    }

    /// The updater of this process, from its executable and environment.
    pub fn from_env() -> Updater {
        let exe = std::env::current_exe().ok();
        let install = exe
            .as_deref()
            .and_then(install_root)
            .filter(|_| std::env::var_os("TITAN_AGENT_NO_UPDATE").is_none())
            .map(|(root, running)| Install { root, running });
        let config = config_dir(
            std::env::var("TITAN_AGENT_CONFIG_DIR").ok().as_deref(),
            std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
            std::env::var("HOME").ok().as_deref(),
        );
        Updater::new(install, config)
    }

    fn new(install: Option<Install>, config_dir: Option<PathBuf>) -> Updater {
        let mut base_url = RELEASES_URL.to_string();
        let mut keys: Vec<String> = accepted_keys().into_iter().map(String::from).collect();
        let mut first_check = FIRST_CHECK;
        // Only a debug build can be pointed at a local server and a test key
        // (the end-to-end tests); a release build ignores these variables.
        #[cfg(debug_assertions)]
        {
            if let Ok(url) = std::env::var("TITAN_AGENT_UPDATE_URL") {
                base_url = url;
            }
            if let Ok(extra) = std::env::var("TITAN_AGENT_UPDATE_KEYS") {
                keys.extend(extra.split(',').map(str::trim).map(String::from));
            }
            if let Some(seconds) = std::env::var("TITAN_AGENT_UPDATE_FIRST_CHECK_S")
                .ok()
                .and_then(|s| s.parse().ok())
            {
                first_check = Duration::from_secs(seconds);
            }
        }
        let auto = config_dir
            .as_deref()
            .map(read_auto)
            .unwrap_or(true);
        let status = UpdateStatus {
            active: install.is_some(),
            installed: env!("CARGO_PKG_VERSION").to_string(),
            auto,
            rolled_back: install
                .as_ref()
                .and_then(|i| read_json(&i.root.join("update/rolled-back.json"))),
            ..UpdateStatus::default()
        };
        Updater {
            install,
            config_dir,
            base_url,
            keys,
            first_check,
            inner: Mutex::new(Inner {
                status,
                prepared: None,
            }),
            wake: Mutex::new(None),
        }
    }

    pub fn is_active(&self) -> bool {
        self.install.is_some()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The status as the clients see it.
    pub fn status(&self, hub: &Hub) -> UpdateStatus {
        let mut status = self.lock().status.clone();
        status.current = self
            .install
            .as_ref()
            .and_then(|i| current_version(&i.root))
            .map(|v| v.to_string());
        let ready = matches!(status.state, UpdateState::Ready | UpdateState::Waiting);
        status.blocking = if ready || self.restart_target().is_some() {
            hub.windows_and_blockers().1
        } else {
            Vec::new()
        };
        status
    }

    fn set(&self, hub: &Hub, change: impl FnOnce(&mut Inner)) {
        change(&mut self.lock());
        hub.broadcast_update(self.status(hub));
    }

    /// Turns the automatic checks on or off and saves it in `update.json`.
    pub fn set_auto(&self, hub: &Hub, auto: bool) -> Result<(), String> {
        let dir = self
            .config_dir
            .as_ref()
            .ok_or("no se sabe dónde guardar la configuración (falta HOME)")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join("update.json");
        std::fs::write(&path, serde_json::json!({ "auto": auto }).to_string())
            .map_err(|e| format!("no se pudo guardar {}: {e}", path.display()))?;
        self.set(hub, |inner| inner.status.auto = auto);
        Ok(())
    }

    /// Forgets the notice of the last rollback.
    pub fn dismiss(&self, hub: &Hub) {
        if let Some(install) = &self.install {
            let _ = std::fs::remove_file(install.root.join("update/rolled-back.json"));
        }
        self.set(hub, |inner| inner.status.rolled_back = None);
    }

    /// «Buscar ahora».
    pub fn request_check(&self) {
        if let Some(wake) = self.wake.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            let _ = wake.send(());
        }
    }

    /// The thread of the updater: prunes old versions, then checks at the
    /// first check and every 6 h (when automatic) or when asked.
    pub fn run(hub: Arc<Hub>) {
        let Some(install) = hub.updater.install.clone() else {
            return;
        };
        let (tx, rx): (Sender<()>, Receiver<()>) = channel();
        *hub.updater.wake.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
        prune(&install.root);
        let mut next = Instant::now() + hub.updater.first_check;
        loop {
            let wait = next.saturating_duration_since(Instant::now());
            let asked = match rx.recv_timeout(wait) {
                Ok(()) => true,
                Err(RecvTimeoutError::Timeout) => false,
                Err(RecvTimeoutError::Disconnected) => return,
            };
            if !asked {
                next = Instant::now() + CHECK_EVERY;
                if !hub.updater.lock().status.auto {
                    continue;
                }
            }
            hub.updater.check(&hub, &install);
            prune(&install.root);
        }
    }

    /// One check: manifest, signature, choice, download and extraction.
    fn check(&self, hub: &Arc<Hub>, install: &Install) {
        if self.lock().status.state == UpdateState::Applying {
            return;
        }
        let previous = self.lock().status.state;
        self.set(hub, |inner| {
            inner.status.state = UpdateState::Checking;
            inner.status.error = None;
        });
        let result = self.find(install);
        let now = now_ms();
        match result {
            Ok(None) => self.set(hub, |inner| {
                inner.status.last_check_ms = Some(now);
                inner.status.state = if inner.prepared.is_some() {
                    previous
                } else {
                    UpdateState::Idle
                };
            }),
            Ok(Some(selected)) => {
                let version = selected.version;
                self.set(hub, |inner| {
                    inner.status.version = Some(version.to_string());
                    inner.status.notes_url = Some(selected.notes_url.clone());
                    inner.status.state = UpdateState::Downloading;
                });
                let prepared = prepare::prepare(&install.root, &selected, fetch::agent());
                self.set(hub, |inner| {
                    inner.status.last_check_ms = Some(now);
                    match prepared {
                        Ok(()) => {
                            inner.prepared = Some(version);
                            inner.status.state = UpdateState::Ready;
                        }
                        Err(error) => {
                            eprintln!("titan-agentd: actualización {version}: {error}");
                            inner.status.state = UpdateState::Failed;
                            inner.status.error = Some(error.to_string());
                        }
                    }
                });
                self.changed(hub);
            }
            Err(error) => {
                eprintln!("titan-agentd: comprobación de actualizaciones: {error}");
                self.set(hub, |inner| {
                    inner.status.last_check_ms = Some(now);
                    inner.status.error = Some(error.to_string());
                    inner.status.state = if inner.prepared.is_some() {
                        previous
                    } else if matches!(error, UpdateError::Download(_)) {
                        // Offline: nothing to show in the header.
                        UpdateState::Idle
                    } else {
                        inner.status.version = None;
                        UpdateState::Failed
                    };
                });
            }
        }
    }

    /// The newer version to install, if any.
    fn find(
        &self,
        install: &Install,
    ) -> Result<Option<titan_agent_core::update::Selected>, UpdateError> {
        let agent = fetch::agent();
        let (manifest, signature) = fetch::manifest(&agent, &self.base_url)?;
        let keys: Vec<&str> = self.keys.iter().map(String::as_str).collect();
        let manifest = verify_manifest(&manifest, &signature, &keys)?;
        let bad = bad_versions(&install.root);
        match select(
            &manifest,
            install.running,
            &bad,
            LINUX_TARGET,
            &self.base_url,
        ) {
            Ok(selected) => Ok(Some(selected)),
            Err(UpdateError::NotNewer(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Sessions or windows changed: apply alone if a version is ready and
    /// it is safe, or say what it waits for.
    pub fn changed(&self, hub: &Arc<Hub>) {
        let Some(to) = self.lock().prepared else {
            return;
        };
        let state = self.lock().status.state;
        if !matches!(state, UpdateState::Ready | UpdateState::Waiting) {
            return;
        }
        let (windows, blockers) = hub.windows_and_blockers();
        if titan_agent_core::update::safe_to_apply(windows, &blockers) {
            self.apply(hub, to, true);
        } else {
            let next = if windows == 0 {
                UpdateState::Waiting
            } else {
                UpdateState::Ready
            };
            if next != state {
                self.set(hub, |inner| inner.status.state = next);
            }
        }
    }

    /// `update_apply`: the prepared version or, with none, a restart into
    /// the installed one when this daemon is another version.
    pub fn apply_now(&self, hub: &Arc<Hub>, force: bool) -> Result<(), (&'static str, String)> {
        if !self.is_active() {
            return Err(inactive());
        }
        let prepared = self.lock().prepared;
        let (to, new_version) = match (prepared, self.restart_target()) {
            (Some(to), _) => (to, true),
            (None, Some(current)) => (current, false),
            (None, None) => {
                return Err(("nothing_to_apply", "no hay ninguna versión preparada".into()));
            }
        };
        let blockers = hub.windows_and_blockers().1;
        if !blockers.is_empty() && !force {
            return Err((
                "confirm_needed",
                format!(
                    "se cerrarán los terminales de {} sesiones trabajando",
                    blockers.len()
                ),
            ));
        }
        let hub = Arc::clone(hub);
        // After the answer reaches the window.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            hub.updater.apply(&hub, to, new_version);
        });
        Ok(())
    }

    /// The installed version, when this daemon is not it.
    fn restart_target(&self) -> Option<Version> {
        let install = self.install.as_ref()?;
        current_version(&install.root).filter(|current| *current != install.running)
    }

    /// Applies `to` and replaces this process with its daemon. Returns only
    /// if that failed.
    fn apply(&self, hub: &Arc<Hub>, to: Version, new_version: bool) {
        let Some(install) = self.install.clone() else {
            return;
        };
        {
            let mut inner = self.lock();
            if inner.status.state == UpdateState::Applying {
                return;
            }
            inner.status.state = UpdateState::Applying;
        }
        hub.broadcast_update(self.status(hub));
        let pending = new_version.then(|| Pending {
            from: install.running.to_string(),
            to: to.to_string(),
            attempts: 0,
        });
        let error = match apply::switch(&install.root, to, pending.as_ref()) {
            Ok(daemon) => {
                eprintln!("titan-agentd: aplicando la versión {to}");
                hub.close_terminals();
                std::thread::sleep(Duration::from_millis(300));
                let error = apply::exec(&daemon);
                apply::undo(&install.root, install.running, pending.is_some());
                error
            }
            Err(error) => error,
        };
        eprintln!("titan-agentd: no se pudo aplicar {to}: {error}");
        self.set(hub, |inner| {
            inner.status.state = UpdateState::Failed;
            inner.status.error = Some(error);
        });
    }
}

fn inactive() -> (&'static str, String) {
    (
        "update_inactive",
        "esta build no se actualiza (no está instalada o TITAN_AGENT_NO_UPDATE)".into(),
    )
}

/// The error of `update_check` on an inactive daemon.
pub fn inactive_error() -> (&'static str, String) {
    inactive()
}

fn read_auto(dir: &Path) -> bool {
    read_json::<serde_json::Value>(&dir.join("update.json"))
        .and_then(|value| value.get("auto").and_then(serde_json::Value::as_bool))
        .unwrap_or(true)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Versions the launcher rolled back from: never installed again.
fn bad_versions(root: &Path) -> Vec<Version> {
    read_json::<Vec<String>>(&root.join("update/bad.json"))
        .unwrap_or_default()
        .iter()
        .filter_map(|v| Version::parse(v))
        .collect()
}

/// The version `current` points to.
pub fn current_version(root: &Path) -> Option<Version> {
    let target = std::fs::read_link(root.join("current")).ok()?;
    Version::parse(target.file_name()?.to_str()?)
}

/// Deletes the versions no longer kept (ADR-0011 §4), only when nothing is
/// pending confirmation.
fn prune(root: &Path) {
    if root.join("update/pending.json").exists() {
        return;
    }
    let Some(current) = current_version(root) else {
        return;
    };
    let installed: Vec<Version> = std::fs::read_dir(root.join("versions"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| Version::parse(entry.file_name().to_str()?))
        .collect();
    for version in versions_to_remove(current, &installed, &bad_versions(root)) {
        let dir = root.join("versions").join(version.to_string());
        if let Err(error) = std::fs::remove_dir_all(&dir) {
            eprintln!("titan-agentd: no se pudo borrar {}: {error}", dir.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_is_on_unless_the_file_says_otherwise() {
        let dir = std::env::temp_dir().join(format!("titan-update-auto-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(read_auto(&dir));
        std::fs::write(dir.join("update.json"), r#"{"auto":false}"#).unwrap();
        assert!(!read_auto(&dir));
        std::fs::write(dir.join("update.json"), "not json").unwrap();
        assert!(read_auto(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn pruning_keeps_current_previous_and_waits_for_confirmation() {
        let root = std::env::temp_dir().join(format!("titan-update-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for v in ["0.5.0", "0.5.1", "0.6.0"] {
            std::fs::create_dir_all(root.join("versions").join(v)).unwrap();
        }
        std::fs::create_dir_all(root.join("update")).unwrap();
        std::os::unix::fs::symlink("versions/0.6.0", root.join("current")).unwrap();
        std::fs::write(root.join("update/pending.json"), "{}").unwrap();
        prune(&root);
        assert!(root.join("versions/0.5.0").exists(), "pending: nothing goes");
        std::fs::remove_file(root.join("update/pending.json")).unwrap();
        prune(&root);
        assert!(!root.join("versions/0.5.0").exists());
        assert!(root.join("versions/0.5.1").exists() && root.join("versions/0.6.0").exists());
        assert_eq!(current_version(&root), Version::parse("0.6.0"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
