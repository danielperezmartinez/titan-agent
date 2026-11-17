#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Repo {
    dir: PathBuf,
}
impl Repo {
    fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("titan-version-scripts-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        Self { dir }
    }
    fn cargo(&self, version: Option<&str>) {
        let text = match version {
            Some(v) => format!("[workspace.package]\nversion = \"{v}\"\n"),
            None => "[workspace.package]\n".into(),
        };
        fs::write(self.dir.join("Cargo.toml"), text).unwrap();
    }
    fn git(&self, args: &[&str]) -> Output {
        let out = Command::new("git")
            .current_dir(&self.dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git fixture setup: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }
    fn script(&self, name: &str, args: &[&str]) -> Output {
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("scripts")
            .join(name);
        Command::new("sh")
            .arg(script)
            .args(args)
            .current_dir(&self.dir)
            .output()
            .unwrap()
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn c9_release_scripts_allow_zero_series_and_reject_one_series_and_nonfinal_versions() {
    // Git operations are confined to this disposable fixture, never the workspace repository.
    let repo = Repo::new();
    repo.git(&["init", "--quiet"]);
    repo.cargo(Some("0.5.9"));
    repo.git(&["add", "Cargo.toml"]);
    repo.git(&[
        "-c",
        "user.name=Acceptance Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=/dev/null",
        "commit",
        "--quiet",
        "-m",
        "fixture base",
    ]);
    let base = String::from_utf8(repo.git(&["rev-parse", "HEAD"]).stdout).unwrap();
    let base = base.trim();
    repo.cargo(Some("0.6.0"));
    assert!(
        repo.script("comprobar-version.sh", &[base])
            .status
            .success()
    );
    let public = repo.script("version-publicable.sh", &[]);
    assert!(public.status.success());
    assert_eq!(String::from_utf8(public.stdout).unwrap(), "0.6.0\n");
    for v in ["0.5.9", "0.5.8"] {
        repo.cargo(Some(v));
        assert_eq!(
            repo.script("comprobar-version.sh", &[base]).status.code(),
            Some(1)
        );
    }
    for v in ["1.0.0", "1.2.3", "2.0.0"] {
        repo.cargo(Some(v));
        let check = repo.script("comprobar-version.sh", &[base]);
        assert_eq!(check.status.code(), Some(1));
        // C9 requires a message, without specifying which output stream for this script.
        let message = format!(
            "{}{}",
            String::from_utf8_lossy(&check.stdout),
            String::from_utf8_lossy(&check.stderr)
        );
        assert!(message.contains("1.x"), "{message}");
    }
    for v in [
        Some("1.0.0"),
        Some("0.6.0-rc.1"),
        Some("0.6.0+build"),
        Some(""),
        None,
    ] {
        repo.cargo(v);
        let public = repo.script("version-publicable.sh", &[]);
        assert_eq!(public.status.code(), Some(1), "version: {v:?}");
        assert!(!public.stderr.is_empty(), "version: {v:?}");
    }
}
