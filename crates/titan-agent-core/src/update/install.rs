//! The per-user install (ADR-0011 §4 and §7): where it lives, which
//! versions to keep, when the daemon may apply a version and what the
//! launcher does with a pending one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Version;
use crate::protocol::TerminalRef;
use crate::session::{Session, SessionState};

/// Name of the install folder under the data folder.
pub const INSTALL_DIR: &str = "titan-agent";
/// Application id: `.desktop`, icon and Wayland `app_id`.
pub const APP_ID: &str = "io.github.danielperezmartinez.titanagent";

/// `update/pending.json`: a version applied but not yet confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub from: String,
    pub to: String,
    pub attempts: u32,
}

/// `update/rolled-back.json`: the launcher went back from `to` to `from`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolledBack {
    pub from: String,
    pub to: String,
}

/// What the launcher does before starting the interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchStep {
    /// Nothing pending: start `current`.
    Run,
    /// Write this pending (one more attempt) and start `current`.
    Count(Pending),
    /// The new version failed to start twice: go back to `from`.
    RollBack { from: String, to: String },
}

/// Starts after two failed ones roll back (ADR-0011 §7).
pub fn launch_step(pending: Option<&Pending>) -> LaunchStep {
    match pending {
        None => LaunchStep::Run,
        Some(p) if p.attempts >= 2 => LaunchStep::RollBack {
            from: p.from.clone(),
            to: p.to.clone(),
        },
        Some(p) => LaunchStep::Count(Pending {
            attempts: p.attempts + 1,
            ..p.clone()
        }),
    }
}

/// Keys of the sessions that keep the daemon from applying: those with a
/// terminal in the daemon that are working. Sorted, without repeats.
pub fn apply_blockers(sessions: &[Session], terminals: &[TerminalRef]) -> Vec<String> {
    let mut keys: Vec<String> = sessions
        .iter()
        .filter(|s| s.state == SessionState::Working)
        .filter(|s| terminals.iter().any(|t| t.session == s.key))
        .map(|s| s.key.clone())
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Applying alone (at window close) needs no window and no blocker.
pub fn safe_to_apply(windows: usize, blockers: &[String]) -> bool {
    windows == 0 && blockers.is_empty()
}

/// The install folder and version of an executable at
/// `<root>/versions/<v>/bin/<file>`, where `<root>` is named
/// `titan-agent`. Anything else (a build tree) has none: its updater is off.
pub fn install_root(exe: &Path) -> Option<(PathBuf, Version)> {
    let bin = exe.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    let version_dir = bin.parent()?;
    let version = Version::parse(version_dir.file_name()?.to_str()?)?;
    let versions = version_dir.parent()?;
    if versions.file_name()? != "versions" {
        return None;
    }
    let root = versions.parent()?;
    (root.file_name()? == INSTALL_DIR).then(|| (root.to_path_buf(), version))
}

/// Versions to delete once nothing is pending: every one below `current`
/// except the newest of them (the previous one), and every bad one that is
/// not `current`. Newer good ones (prepared) stay. Sorted.
pub fn versions_to_remove(
    current: Version,
    installed: &[Version],
    bad: &[Version],
) -> Vec<Version> {
    let previous = installed.iter().filter(|v| **v < current).max().copied();
    let mut remove: Vec<Version> = installed
        .iter()
        .copied()
        .filter(|v| *v != current)
        .filter(|v| bad.contains(v) || (*v < current && Some(*v) != previous))
        .collect();
    remove.sort();
    remove.dedup();
    remove
}

/// The data folder: `$XDG_DATA_HOME` if absolute, else `$HOME/.local/share`.
pub fn data_home(home: Option<&str>, xdg_data_home: Option<&str>) -> Option<PathBuf> {
    match xdg_data_home.filter(|dir| dir.starts_with('/')) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => home
            .filter(|h| !h.is_empty())
            .map(|h| Path::new(h).join(".local/share")),
    }
}

/// The settings folder of titan-agent: `$TITAN_AGENT_CONFIG_DIR`, else
/// `$XDG_CONFIG_HOME/titan-agent` (if absolute), else
/// `$HOME/.config/titan-agent`.
pub fn config_dir(
    explicit: Option<&str>,
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> Option<PathBuf> {
    if let Some(dir) = explicit.filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    match xdg_config_home.filter(|dir| dir.starts_with('/')) {
        Some(dir) => Some(Path::new(dir).join(INSTALL_DIR)),
        None => home
            .filter(|h| !h.is_empty())
            .map(|h| Path::new(h).join(".config").join(INSTALL_DIR)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn the_launcher_counts_two_attempts_and_then_rolls_back() {
        assert_eq!(launch_step(None), LaunchStep::Run);
        let mut pending = Pending {
            from: "0.6.0".into(),
            to: "0.6.1".into(),
            attempts: 0,
        };
        for expected in [1, 2] {
            match launch_step(Some(&pending)) {
                LaunchStep::Count(next) => {
                    assert_eq!(next.attempts, expected);
                    pending = next;
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(
            launch_step(Some(&pending)),
            LaunchStep::RollBack {
                from: "0.6.0".into(),
                to: "0.6.1".into()
            }
        );
    }

    #[test]
    fn only_executables_inside_versions_are_installed() {
        let (root, version) = install_root(Path::new(
            "/h/.local/share/titan-agent/versions/0.6.1/bin/titan-agentd",
        ))
        .unwrap();
        assert_eq!(root, Path::new("/h/.local/share/titan-agent"));
        assert_eq!(version, v("0.6.1"));
        for build in [
            "/w/titan-agent/target/debug/titan-agentd",
            "/h/other/versions/0.6.1/bin/titan-agentd",
            "/h/titan-agent/versions/main/bin/titan-agentd",
            "/h/titan-agent/versions/0.6.1/titan-agentd",
        ] {
            assert!(install_root(Path::new(build)).is_none(), "{build}");
        }
    }

    #[test]
    fn current_the_previous_and_prepared_versions_are_kept() {
        let all = [v("0.5.0"), v("0.5.2"), v("0.6.0"), v("0.6.1"), v("0.7.0")];
        assert_eq!(
            versions_to_remove(v("0.6.1"), &all, &[v("0.7.0")]),
            [v("0.5.0"), v("0.5.2"), v("0.7.0")]
        );
        assert_eq!(
            versions_to_remove(v("0.6.1"), &all, &[]),
            [v("0.5.0"), v("0.5.2")]
        );
        assert!(versions_to_remove(v("0.5.0"), &[v("0.5.0")], &[v("0.5.0")]).is_empty());
    }

    #[test]
    fn folders_follow_xdg_with_home_as_fallback() {
        assert_eq!(
            data_home(Some("/h"), Some("relative")).unwrap(),
            Path::new("/h/.local/share")
        );
        assert_eq!(data_home(Some("/h"), Some("/d")).unwrap(), Path::new("/d"));
        assert_eq!(
            config_dir(None, None, Some("/h")).unwrap(),
            Path::new("/h/.config/titan-agent")
        );
        assert_eq!(
            config_dir(Some("/c"), Some("/x"), Some("/h")).unwrap(),
            Path::new("/c")
        );
        assert_eq!(
            config_dir(None, Some("/x"), Some("/h")).unwrap(),
            Path::new("/x/titan-agent")
        );
    }
}
