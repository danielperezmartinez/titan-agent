#![cfg(unix)]

use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture {
    dir: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "titan-launcher-acceptance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        for p in ["home", "data", "tools", "runtime", "config"] {
            fs::create_dir(dir.join(p)).unwrap();
        }
        Self { dir }
    }
    fn root(&self) -> PathBuf {
        self.dir.join("data/titan-agent")
    }
    fn command(&self, exe: &Path) -> Command {
        let mut c = Command::new(exe);
        let mut paths = vec![self.dir.join("tools")];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        c.env("HOME", self.dir.join("home"))
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("XDG_CONFIG_HOME", self.dir.join("config"))
            .env("XDG_RUNTIME_DIR", self.dir.join("runtime"))
            .env("TITAN_AGENTD_SOCKET", self.dir.join("runtime/custom.sock"))
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("ACCEPTANCE_LOG", self.dir.join("launch.log"))
            .env("ACCEPTANCE_NOTICE", self.dir.join("notice.log"));
        c
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(Path::new(env!("CARGO_BIN_EXE_titan-agent-launcher")))
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn script(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn succeeds(output: Output) {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn c6_launcher_counts_two_failures_then_rolls_back_and_executes_previous_version() {
    let f = Fixture::new();
    let root = f.root();
    fs::create_dir_all(root.join("update")).unwrap();
    script(
        &root.join("versions/0.6.0/bin/titan-agent"),
        "#!/bin/sh\nprintf 'old\\n%s\\n' \"$TITAN_AGENT_LAUNCHER\" >> \"$ACCEPTANCE_LOG\"\nprintf '%s\\n' \"$@\" >> \"$ACCEPTANCE_LOG\"\n",
    );
    script(
        &root.join("versions/0.6.1/bin/titan-agent"),
        "#!/bin/sh\nprintf 'new\\n' >> \"$ACCEPTANCE_LOG\"\nexit 17\n",
    );
    script(
        &f.dir.join("tools/notify-send"),
        "#!/bin/sh\nprintf '%s\\n' \"$#\" \"$@\" > \"$ACCEPTANCE_NOTICE\"\n",
    );
    symlink("versions/0.6.1", root.join("current")).unwrap();
    fs::write(
        root.join("update/pending.json"),
        json!({"from":"0.6.0","to":"0.6.1","attempts":0}).to_string(),
    )
    .unwrap();
    // A preexisting bad entry must not be duplicated during rollback.
    fs::write(root.join("update/bad.json"), "[\"0.5.9\",\"0.6.1\"]").unwrap();
    for attempt in 1..=2 {
        assert_eq!(f.run(&[]).status.code(), Some(17));
        assert_eq!(
            read_json(&root.join("update/pending.json")),
            json!({"from":"0.6.0","to":"0.6.1","attempts":attempt})
        );
        assert_eq!(
            fs::read_link(root.join("current")).unwrap(),
            Path::new("versions/0.6.1")
        );
    }
    succeeds(f.run(&[]));
    assert_eq!(
        fs::read_link(root.join("current")).unwrap(),
        Path::new("versions/0.6.0")
    );
    assert!(!root.join("update/pending.json").exists());
    assert_eq!(
        read_json(&root.join("update/rolled-back.json")),
        json!({"from":"0.6.0","to":"0.6.1"})
    );
    let bad = read_json(&root.join("update/bad.json"));
    let bad = bad.as_array().unwrap();
    assert_eq!(bad.len(), 2);
    assert!(bad.contains(&json!("0.5.9")) && bad.contains(&json!("0.6.1")));
    assert_eq!(
        fs::read_to_string(f.dir.join("notice.log")).unwrap(),
        "2\ntitan-agent\nLa versión 0.6.1 no arrancó; se ha vuelto a la 0.6.0.\n"
    );
    assert_eq!(
        fs::read_to_string(f.dir.join("launch.log")).unwrap(),
        format!(
            "new\nnew\nold\n{}\n\n",
            root.join("titan-agent-launcher").display()
        )
    );
    // With no pending record, a normal launch still executes current.
    succeeds(f.run(&[]));
    fs::remove_file(root.join("versions/0.6.0/bin/titan-agent")).unwrap();
    let missing = f.run(&[]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(!missing.stderr.is_empty());
}

#[test]
fn c7_install_is_idempotent_and_uninstall_preserves_configuration_and_foreign_links() {
    let f = Fixture::new();
    let binary = Path::new(env!("CARGO_BIN_EXE_titan-agent-launcher"));
    let version = env!("CARGO_PKG_VERSION");
    let version_output = f.run(&["--version"]);
    assert!(version_output.status.success());
    assert_eq!(
        String::from_utf8(version_output.stdout).unwrap(),
        format!("titan-agent-launcher {version}\n")
    );
    assert_eq!(f.run(&["unknown-command"]).status.code(), Some(2));
    let bundle = f.dir.join("bundle");
    fs::create_dir_all(bundle.join("bin")).unwrap();
    fs::create_dir_all(bundle.join("share")).unwrap();
    fs::copy(binary, bundle.join("bin/titan-agent-launcher")).unwrap();
    for name in ["titan-agent", "titan-agentd"] {
        script(&bundle.join("bin").join(name), "#!/bin/sh\nexit 0\n");
    }
    let install = || {
        f.command(&bundle.join("bin/titan-agent-launcher"))
            .arg("install")
            .output()
            .unwrap()
    };
    assert_eq!(install().status.code(), Some(1));
    assert!(!f.root().join("current").exists());
    assert!(!f.root().join("versions").join(version).exists());
    assert!(!f.dir.join("home/.local/bin/titan-agent").exists());
    script(&bundle.join("bin/titan-team"), "#!/bin/sh\nexit 0\n");
    fs::write(bundle.join("share/sample-icon.svg"), "sample icon bytes").unwrap();
    succeeds(install());
    let root = f.root();
    let installed = root.join("versions").join(version);
    assert_eq!(
        fs::read_link(root.join("current")).unwrap(),
        PathBuf::from(format!("versions/{version}"))
    );
    assert_eq!(
        fs::read_link(f.dir.join("home/.local/bin/titan-agent")).unwrap(),
        root.join("titan-agent-launcher")
    );
    assert_eq!(
        fs::read_link(f.dir.join("home/.local/bin/titan-team")).unwrap(),
        root.join("current/bin/titan-team")
    );
    for name in [
        "titan-agent",
        "titan-agentd",
        "titan-team",
        "titan-agent-launcher",
    ] {
        assert_eq!(
            fs::read(installed.join("bin").join(name)).unwrap(),
            fs::read(bundle.join("bin").join(name)).unwrap()
        );
    }
    assert_eq!(
        fs::read(root.join("titan-agent-launcher")).unwrap(),
        fs::read(binary).unwrap()
    );
    assert_eq!(
        fs::read(installed.join("share/sample-icon.svg")).unwrap(),
        b"sample icon bytes"
    );
    let desktop = f
        .dir
        .join("data/applications/io.github.danielperezmartinez.titanagent.desktop");
    let before = fs::read_to_string(&desktop).unwrap();
    assert!(before.lines().any(|line| line
        == format!(
            "Exec={}/.local/bin/titan-agent",
            f.dir.join("home").display()
        )));
    assert!(
        before
            .lines()
            .any(|line| line == "StartupWMClass=io.github.danielperezmartinez.titanagent")
    );
    succeeds(install());
    assert_eq!(fs::read_to_string(&desktop).unwrap(), before);
    // Reinstalling an existing version must never overwrite its files.
    fs::write(bundle.join("share/sample-icon.svg"), "changed source").unwrap();
    succeeds(install());
    assert_eq!(
        fs::read(installed.join("share/sample-icon.svg")).unwrap(),
        b"sample icon bytes"
    );
    for config in [
        f.dir.join("config/titan-agent"),
        f.dir.join("home/.config/titan-agent"),
    ] {
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("keep"), "user settings").unwrap();
    }
    let team = f.dir.join("home/.local/bin/titan-team");
    fs::remove_file(&team).unwrap();
    symlink(f.dir.join("foreign-team"), &team).unwrap();
    succeeds(f.run(&["uninstall"]));
    assert!(!root.exists());
    assert!(!desktop.exists());
    assert!(fs::symlink_metadata(f.dir.join("home/.local/bin/titan-agent")).is_err());
    assert_eq!(fs::read_link(&team).unwrap(), f.dir.join("foreign-team"));
    for config in [
        f.dir.join("config/titan-agent"),
        f.dir.join("home/.config/titan-agent"),
    ] {
        assert_eq!(
            fs::read_to_string(config.join("keep")).unwrap(),
            "user settings"
        );
    }
    succeeds(f.run(&["uninstall"]));
}
