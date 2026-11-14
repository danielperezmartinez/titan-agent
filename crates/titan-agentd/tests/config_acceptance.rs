use std::{env, os::unix::fs::MetadataExt, path::PathBuf, process::Command, time::Duration};
use titan_agentd::config::{Config, default_socket_path};

// Each environment scenario runs in a child, avoiding unsafe environment mutation
// and interference with any other tests in the Rust 2024 workspace.
#[test]
fn configuration_environment_scenarios() {
    for scenario in ["defaults", "environment", "arguments", "empty-runtime"] {
        let mut command = Command::new(env::current_exe().unwrap());
        command
            .args(["--exact", "configuration_child", "--nocapture"])
            .env("TITAN_TEST_CONFIG_CASE", scenario);
        for name in [
            "TITAN_AGENTD_SOCKET",
            "TITAN_AGENTD_CLAUDE",
            "TITAN_AGENTD_CODEX",
            "TITAN_AGENTD_GIT",
            "XDG_RUNTIME_DIR",
        ] {
            command.env_remove(name);
        }
        if scenario != "defaults" {
            command
                .env("XDG_RUNTIME_DIR", "/tmp/titan-fixture-runtime")
                .env("TITAN_AGENTD_SOCKET", "/tmp/titan-fixture-env.sock")
                .env("TITAN_AGENTD_CLAUDE", "fixture-claude")
                .env("TITAN_AGENTD_CODEX", "off")
                .env("TITAN_AGENTD_GIT", "off");
        }
        if scenario == "empty-runtime" {
            command.env("XDG_RUNTIME_DIR", "");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{scenario}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn configuration_child() {
    let Ok(scenario) = env::var("TITAN_TEST_CONFIG_CASE") else {
        return;
    };
    let args = if scenario == "arguments" {
        vec!["--socket".into(), "/tmp/titan-fixture-arg.sock".into()]
    } else {
        vec![]
    };
    let config = Config::from_env_and_args(args).unwrap();
    assert_eq!(config.poll_subscribed, Duration::from_secs(2));
    assert_eq!(config.poll_idle, Duration::from_secs(15));
    if scenario == "defaults" {
        assert_eq!(config.claude_program, "claude");
        assert_eq!(config.codex_program.as_deref(), Some("codex"));
        assert_eq!(config.git_program.as_deref(), Some("git"));
        assert_eq!(config.socket, default_socket_path());
        let explicit = Config::new(PathBuf::from("/tmp/titan-fixture-explicit.sock"));
        assert_eq!(
            explicit.socket,
            PathBuf::from("/tmp/titan-fixture-explicit.sock")
        );
        assert_eq!(explicit.claude_program, "claude");
        assert_eq!(explicit.codex_program.as_deref(), Some("codex"));
        assert_eq!(explicit.git_program.as_deref(), Some("git"));
        assert_eq!(explicit.poll_idle, Duration::from_secs(15));
        assert_eq!(explicit.poll_subscribed, Duration::from_secs(2));
    } else {
        assert_eq!(config.claude_program, "fixture-claude");
        assert!(config.codex_program.is_none());
        assert!(config.git_program.is_none());
        let expected = if scenario == "arguments" {
            "/tmp/titan-fixture-arg.sock"
        } else {
            "/tmp/titan-fixture-env.sock"
        };
        assert_eq!(config.socket, PathBuf::from(expected));
    }
    if scenario == "defaults" || scenario == "empty-runtime" {
        let uid = std::fs::metadata("/proc/self").unwrap().uid();
        assert_eq!(
            default_socket_path(),
            env::temp_dir().join(format!("titan-agent-{uid}/agentd.sock"))
        );
    } else {
        assert_eq!(
            default_socket_path(),
            PathBuf::from("/tmp/titan-fixture-runtime/titan-agent/agentd.sock")
        );
    }
    for args in [vec!["--unknown".into()], vec!["--socket".into()]] {
        assert!(
            !Config::from_env_and_args(args)
                .err()
                .expect("must reject invalid arguments")
                .is_empty()
        );
    }
}

#[test]
fn binary_version_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_titan-agentd"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        concat!("titan-agentd ", env!("CARGO_PKG_VERSION"))
    );
}
