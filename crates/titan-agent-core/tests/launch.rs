use titan_agent_core::launch::{
    LaunchDefaults, LaunchError, LaunchSpec, MAX_BRIEF_BYTES, parse_claude_bg_output, session_name,
    shell_quote,
};
use titan_agent_core::session::AgentCli;

fn claude(brief: &str) -> LaunchSpec {
    LaunchSpec {
        agent: AgentCli::ClaudeCode,
        model: "opus".into(),
        effort: "high".into(),
        access: "manual".into(),
        cwd: "/srv/demo/dev/kite".into(),
        brief: brief.into(),
    }
}

fn codex(brief: &str) -> LaunchSpec {
    LaunchSpec {
        agent: AgentCli::Codex,
        model: "gpt-6-luna".into(),
        effort: "medium".into(),
        access: "workspace-write".into(),
        cwd: "/srv/demo/dev/kite".into(),
        brief: brief.into(),
    }
}

#[test]
fn an_empty_or_blank_brief_is_refused() {
    assert_eq!(claude("").validate(), Err(LaunchError::EmptyBrief));
    assert_eq!(claude("  \n\t ").validate(), Err(LaunchError::EmptyBrief));
    assert_eq!(codex("").validate(), Err(LaunchError::EmptyBrief));
    assert_eq!(LaunchError::EmptyBrief.code(), "empty_brief");
}

#[test]
fn a_brief_that_cannot_be_one_argument_is_refused() {
    assert!(matches!(
        claude("hola\0adiós").validate(),
        Err(LaunchError::BadBrief(_))
    ));
    let long = "a".repeat(MAX_BRIEF_BYTES + 1);
    assert!(matches!(
        claude(&long).validate(),
        Err(LaunchError::BadBrief(_))
    ));
}

#[test]
fn only_offered_options_are_accepted() {
    let mut spec = claude("Revisa el README");
    assert_eq!(spec.validate(), Ok(()));
    spec.model = "--dangerously-skip-permissions".into();
    assert!(matches!(
        spec.validate(),
        Err(LaunchError::InvalidOption(_))
    ));

    let mut spec = claude("Revisa el README");
    spec.access = "bypassPermissions".into();
    assert!(matches!(
        spec.validate(),
        Err(LaunchError::InvalidOption(_))
    ));

    let mut spec = codex("Revisa el README");
    spec.access = "danger-full-access".into();
    assert!(matches!(
        spec.validate(),
        Err(LaunchError::InvalidOption(_))
    ));

    let mut spec = codex("Revisa el README");
    spec.effort = "ultra".into();
    assert!(matches!(
        spec.validate(),
        Err(LaunchError::InvalidOption(_))
    ));

    let mut spec = codex("Revisa el README");
    spec.cwd = "dev/kite".into();
    assert!(matches!(
        spec.validate(),
        Err(LaunchError::InvalidOption(_))
    ));
}

#[test]
fn claude_runs_in_the_background_with_the_chosen_options() {
    let argv = claude("Revisa el README y dime qué falta").argv();
    assert_eq!(
        argv,
        [
            "claude",
            "--bg",
            "-n",
            "Revisa el README y dime qué",
            "--model",
            "opus",
            "--effort",
            "high",
            "--permission-mode",
            "manual",
            "Revisa el README y dime qué falta",
        ]
    );
}

#[test]
fn codex_runs_its_own_tui_with_the_chosen_options() {
    let argv = codex("Añade un test").argv();
    assert_eq!(
        argv,
        [
            "codex",
            "--remote",
            "unix://",
            "-m",
            "gpt-6-luna",
            "-c",
            "model_reasoning_effort=medium",
            "-s",
            "workspace-write",
            "Añade un test",
        ]
    );
}

#[test]
fn the_brief_reaches_the_cli_byte_for_byte() {
    let brief = "  Primera línea\n\n- un punto con \"comillas\", $HOME y `tildes` ñ \n";
    for spec in [claude(brief), codex(brief)] {
        assert_eq!(spec.argv().last().map(String::as_str), Some(brief));
    }
}

#[test]
fn a_brief_starting_with_a_dash_is_not_an_option() {
    let argv = claude("-v explica esto").argv();
    assert_eq!(argv[argv.len() - 2], "--");
    assert_eq!(argv[argv.len() - 1], "-v explica esto");
    let argv = codex("--help").argv();
    assert_eq!(&argv[argv.len() - 2..], ["--", "--help"]);
}

#[test]
fn the_shown_command_is_the_argv_quoted_for_a_shell() {
    let spec = claude("Revisa el README");
    assert_eq!(
        spec.command_line(Some("/srv/demo")),
        "cd ~/dev/kite && claude --bg -n \"Revisa el README\" --model opus --effort high \
         --permission-mode manual \"Revisa el README\""
    );
    let spec = codex("Añade un test");
    assert_eq!(
        spec.command_line(None),
        "cd /srv/demo/dev/kite && codex --remote unix:// -m gpt-6-luna -c model_reasoning_effort=medium \
         -s workspace-write \"Añade un test\""
    );
}

#[test]
fn folders_with_spaces_keep_the_tilde_outside_the_quotes() {
    let mut spec = codex("hola");
    spec.cwd = "/srv/demo/mis proyectos/kite".into();
    assert!(
        spec.command_line(Some("/srv/demo"))
            .starts_with("cd ~/\"mis proyectos/kite\" && ")
    );
    spec.cwd = "/srv/demo".into();
    assert!(
        spec.command_line(Some("/srv/demo"))
            .starts_with("cd ~ && ")
    );
}

#[test]
fn the_preview_shows_a_placeholder_until_there_is_a_brief() {
    let spec = claude("   ");
    let preview = spec.command_preview(Some("/srv/demo"));
    assert!(preview.ends_with("--permission-mode manual \"<tu encargo>\""));
    assert!(preview.contains("-n \"<primeras palabras>\""));
    let spec = claude("Haz algo");
    assert_eq!(
        spec.command_preview(Some("/srv/demo")),
        spec.command_line(Some("/srv/demo"))
    );
}

#[test]
fn shell_quoting_is_exact() {
    assert_eq!(shell_quote("opus"), "opus");
    assert_eq!(
        shell_quote("model_reasoning_effort=high"),
        "model_reasoning_effort=high"
    );
    assert_eq!(shell_quote("dos palabras"), "\"dos palabras\"");
    assert_eq!(shell_quote(""), "\"\"");
    assert_eq!(shell_quote("cuesta $5"), "'cuesta $5'");
    assert_eq!(shell_quote("it's \"x\""), "'it'\\''s \"x\"'");
    assert_eq!(shell_quote("a`b`"), "'a`b`'");
}

#[test]
fn session_names_come_from_the_first_words() {
    assert_eq!(
        session_name("Revisa el README y dime qué falta"),
        "Revisa el README y dime qué"
    );
    assert_eq!(session_name("\n\n  Corto.\nsegunda línea"), "Corto");
    assert_eq!(session_name("   "), "nueva sesión");
    let long = session_name(&"palabralarguísima".repeat(5));
    assert_eq!(long.chars().count(), 48);
    assert!(
        session_name("una dos tres cuatro cinco seis siete ocho nueve diez once")
            .chars()
            .count()
            <= 48
    );
}

#[test]
fn reads_the_id_printed_by_claude_bg() {
    let stdout = "backgrounded · f3c0ab80 · prueba: formato\n  claude agents             list sessions\n  claude attach f3c0ab80    open in this terminal\n";
    assert_eq!(parse_claude_bg_output(stdout).as_deref(), Some("f3c0ab80"));
    assert_eq!(parse_claude_bg_output("nothing here"), None);
}

#[test]
fn defaults_fall_back_to_offered_values() {
    let defaults = LaunchDefaults::parse("not json");
    assert_eq!(defaults, LaunchDefaults::default());

    let saved = r#"{"agent":"codex","claude":{"model":"opus-9","effort":"max","access":"plan"},
                    "codex":{"model":"gpt-6.1-sol","effort":"low","access":"danger-full-access"}}"#;
    let defaults = LaunchDefaults::parse(saved);
    assert_eq!(defaults.agent, AgentCli::Codex);
    assert_eq!(defaults.claude.model, "opus");
    assert_eq!(defaults.claude.effort, "max");
    assert_eq!(defaults.claude.access, "plan");
    assert_eq!(defaults.codex.model, "gpt-6.1-sol");
    assert_eq!(defaults.codex.access, "workspace-write");

    let again = LaunchDefaults::parse(&defaults.to_json());
    assert_eq!(again, defaults);
}

#[test]
fn the_fingerprint_tells_launches_apart() {
    assert_eq!(claude("a").fingerprint(), claude("a").fingerprint());
    assert_ne!(claude("a").fingerprint(), claude("b").fingerprint());
    assert_ne!(claude("a").fingerprint(), codex("a").fingerprint());
}

fn thread(
    key: &str,
    name: &str,
    cwd: &str,
    started_at_ms: i64,
) -> titan_agent_core::session::Session {
    use titan_agent_core::project::{project_name, project_root};
    use titan_agent_core::session::{Session, SessionKind, SessionState};
    Session {
        key: key.into(),
        agent: AgentCli::Codex,
        cli_id: key.trim_start_matches("cx:").into(),
        name: name.into(),
        cwd: cwd.into(),
        project_root: project_root(cwd),
        project: project_name(&project_root(cwd)),
        kind: SessionKind::Client,
        state: SessionState::Working,
        model: None,
        effort: None,
        role: None,
        parent: None,
        started_at_ms: Some(started_at_ms),
        state_since_ms: None,
        pid: None,
        attach: None,
        stoppable: false,
        launched_here: false,
    }
}

#[test]
fn a_launched_tui_is_matched_with_the_new_thread_of_its_folder() {
    use titan_agent_core::launch::{LaunchedTui, pick_launched_thread};
    let launched = LaunchedTui {
        cwd: "/srv/demo/kite".into(),
        brief: "Añade un test al parser".into(),
        launched_ms: 1_000_000,
    };
    let sessions = vec![
        thread(
            "cx:old",
            "Añade un test al parser",
            "/srv/demo/kite",
            1_000_500,
        ),
        thread("cx:other", "Añade algo", "/srv/demo/otro", 1_000_600),
        thread("cx:stale", "Añade un test", "/srv/demo/kite", 100_000),
        thread(
            "cx:new",
            "Revisa",
            "/srv/demo/kite/.claude/worktrees/x",
            1_001_000,
        ),
        thread("cx:named", "Añade un test al…", "/srv/demo/kite", 1_002_000),
    ];
    // `old` existed at launch time; `other` is another project; `stale`
    // started long before; `named` matches the brief.
    let taken = |key: &str| key == "cx:old";
    let picked = pick_launched_thread(&launched, &sessions, &taken).unwrap();
    assert_eq!(picked.key, "cx:named");

    let taken = |key: &str| key == "cx:old" || key == "cx:named";
    let picked = pick_launched_thread(&launched, &sessions, &taken).unwrap();
    assert_eq!(picked.key, "cx:new");

    let taken = |key: &str| key != "cx:other";
    assert!(pick_launched_thread(&launched, &sessions, &taken).is_none());
}
