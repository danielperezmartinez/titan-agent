//! Acceptance contract for user-authored launch commands.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use titan_agent_core::launch::{LaunchSpec, efforts, permissions};
use titan_agent_core::session::AgentCli;

fn spec(agent: AgentCli) -> LaunchSpec {
    LaunchSpec {
        agent,
        cwd: "/srv/example/kite/".into(),
        model: "opus[1m]".into(),
        effort: "high".into(),
        permission: if agent == AgentCli::ClaudeCode {
            "plan"
        } else {
            "workspace-write"
        }
        .into(),
        brief: "  \n  Repair the example\nKeep this second line.  ".into(),
    }
}

#[test]
fn official_argv_preserves_the_brief_and_names_use_unicode_characters() {
    let claude = spec(AgentCli::ClaudeCode);
    assert_eq!(claude.session_name(), "kite: Repair the example");
    assert_eq!(
        claude.argv(),
        vec![
            "claude",
            "--bg",
            "-n",
            "kite: Repair the example",
            "--model",
            "opus[1m]",
            "--effort",
            "high",
            "--permission-mode",
            "plan",
            "--",
            &claude.brief
        ]
    );
    let codex = spec(AgentCli::Codex);
    assert_eq!(
        codex.argv(),
        vec![
            "codex",
            "-m",
            "opus[1m]",
            "-c",
            "model_reasoning_effort=high",
            "-s",
            "workspace-write",
            "--",
            &codex.brief
        ]
    );
    let mut named = claude.clone();
    named.brief = format!("\n  {}  \nignored", "界".repeat(40));
    assert_eq!(named.session_name(), format!("kite: {}", "界".repeat(40)));
    named.brief = format!("{}x", "界".repeat(40));
    assert_eq!(named.session_name(), format!("kite: {}…", "界".repeat(40)));
}

#[test]
fn validation_enforces_option_allowlists_byte_limits_and_first_error_order() {
    assert_eq!(
        efforts(AgentCli::ClaudeCode),
        &["low", "medium", "high", "xhigh", "max"]
    );
    assert_eq!(
        efforts(AgentCli::Codex),
        &["low", "medium", "high", "xhigh"]
    );
    assert_eq!(
        permissions(AgentCli::ClaudeCode),
        &["manual", "plan", "acceptEdits", "auto"]
    );
    assert_eq!(
        permissions(AgentCli::Codex),
        &["read-only", "workspace-write"]
    );
    let base = spec(AgentCli::ClaudeCode);
    let check = |s: &LaunchSpec, code| assert_eq!(s.validate().unwrap_err().code(), code);
    let mut s = base.clone();
    s.brief = " \n\t".into();
    s.cwd = "relative".into();
    s.model = "-bad".into();
    check(&s, "empty_brief");
    s.brief = " ".repeat(32_001);
    check(&s, "empty_brief");
    s.brief = format!("{}\0", "é".repeat(16_000));
    check(&s, "brief_too_long");
    s.brief = "hello\0".into();
    check(&s, "bad_brief");
    s.brief = "hello".into();
    check(&s, "bad_folder");
    s.cwd = base.cwd.clone();
    check(&s, "bad_option");
    for model in ["", "-opus", "has space", "opus;echo", "é", "opus/alias"] {
        let mut s = base.clone();
        s.model = model.into();
        check(&s, "bad_option");
    }
    for model in [
        "opus",
        "haiku",
        "claude-opus-5-5",
        "opus[1m]",
        "gpt-6-luna",
        "gpt-6.1-sol",
        "alias:v1",
    ] {
        let mut s = base.clone();
        s.model = model.into();
        assert!(s.validate().is_ok());
    }
    for agent in [AgentCli::ClaudeCode, AgentCli::Codex] {
        for permission in [
            "bypassPermissions",
            "dontAsk",
            "danger-full-access",
            "unknown",
        ] {
            let mut s = spec(agent);
            s.permission = permission.into();
            check(&s, "bad_option");
        }
        let mut s = spec(agent);
        s.effort = "unknown".into();
        check(&s, "bad_option");
        for effort in efforts(agent) {
            for permission in permissions(agent) {
                let mut s = spec(agent);
                s.effort = (*effort).into();
                s.permission = (*permission).into();
                assert!(s.validate().is_ok());
            }
        }
    }
    let mut s = spec(AgentCli::Codex);
    s.effort = "max".into();
    check(&s, "bad_option");
    let mut s = base;
    s.brief = "é".repeat(16_000);
    assert!(s.validate().is_ok());
    s.brief.push('x');
    check(&s, "brief_too_long");
}

#[test]
fn displayed_posix_command_executes_exact_arguments_in_the_selected_folder() {
    let dir = std::env::temp_dir().join(format!("titan-launch-quote-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cwd = dir.join("kite's workspace");
    std::fs::create_dir_all(&cwd).unwrap();
    let record = dir.join("record");
    for agent in [AgentCli::ClaudeCode, AgentCli::Codex] {
        let mut s = spec(agent);
        s.cwd = cwd.to_str().unwrap().into();
        s.brief =
            "  -literal ' \" $HOME $(touch injected) `touch injected` ; * \\\nsecond line  ".into();
        let argv = s.argv();
        let fake = dir.join(&argv[0]);
        std::fs::write(
            &fake,
            "#!/bin/sh\nprintf '%s\\0' \"$PWD\" \"$0\" \"$@\" > \"$RECORD\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        // This independent encoder fixes the exact display format, including apostrophes.
        let quote = |v: &str| {
            if !v.is_empty()
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_./=:,@%+-".contains(&b))
            {
                v.to_owned()
            } else {
                format!("'{}'", v.replace('\'', "'\\''"))
            }
        };
        let expected = format!(
            "cd {} && {}",
            quote(&s.cwd),
            argv.iter().map(|v| quote(v)).collect::<Vec<_>>().join(" ")
        );
        assert_eq!(s.command_line(), expected);
        let status = Command::new("/bin/sh")
            .arg("-c")
            .arg(s.command_line())
            .env("PATH", &dir)
            .env("RECORD", &record)
            .status()
            .unwrap();
        assert!(status.success());
        let bytes = std::fs::read(&record).unwrap();
        let fields: Vec<_> = bytes.split(|b| *b == 0).collect();
        assert_eq!(fields[0], s.cwd.as_bytes());
        assert_eq!(fields[1], fake.as_os_str().as_encoded_bytes());
        let expected_args: Vec<_> = argv[1..].iter().map(|a| a.as_bytes()).collect();
        assert_eq!(&fields[2..fields.len() - 1], expected_args.as_slice());
        assert!(!cwd.join("injected").exists());
    }
    std::fs::remove_dir_all(dir).unwrap();
}
