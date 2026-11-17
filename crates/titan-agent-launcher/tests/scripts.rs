//! Own test of `scripts/comprobar-version.sh` (the acceptance tests are in
//! `scripts_acceptance.rs`): a 1.x version is refused even when the base
//! commit has no `Cargo.toml` to compare with.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

#[test]
fn one_point_x_is_refused_without_a_base_version_too() {
    let dir = std::env::temp_dir().join(format!("titan-version-nobase-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(&dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    git(&["init", "--quiet"]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "core.hooksPath=/dev/null",
        "commit",
        "--quiet",
        "--allow-empty",
        "-m",
        "no cargo yet",
    ]);
    let base = git(&["rev-parse", "HEAD"]);
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/comprobar-version.sh");
    let check = |version: &str| {
        std::fs::write(
            dir.join("Cargo.toml"),
            format!("[workspace.package]\nversion = \"{version}\"\n"),
        )
        .unwrap();
        Command::new("sh")
            .arg(&script)
            .arg(base.trim())
            .current_dir(&dir)
            .output()
            .unwrap()
            .status
            .code()
    };
    assert_eq!(check("1.0.0"), Some(1));
    assert_eq!(check("0.6.0"), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}
