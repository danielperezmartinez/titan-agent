use titan_agent::theme;
use titan_agent::view_model::{
    Detail, StopAction, Talk, agent_color, agent_line, attach_line, detail, grid_size,
    header_counts, monitor_text, not_attachable_text, pill, state_style, stop_action,
};
use titan_agent_core::session::{
    AgentCli, Project, Session, SessionKey, SessionKind, SessionState,
};

fn project() -> Project {
    Project {
        name: "Orchid".into(),
        root: "/fictional/orchid".into(),
    }
}

fn session(cli: AgentCli, id: &str, name: &str) -> Session {
    Session {
        key: SessionKey { cli, id: id.into() },
        name: name.into(),
        kind: SessionKind::Interactive,
        state: SessionState::Working,
        cwd: "/srv/fictional/orchid/branch".into(),
        project: project(),
        model: None,
        parent: None,
        started_at_ms: None,
        state_since_ms: None,
        attach: None,
    }
}

fn talk(direction: &'static str, who: &str, color: u32, what: &str) -> Talk {
    Talk {
        direction,
        who: who.into(),
        color,
        what: what.into(),
    }
}

#[test]
fn theme_tokens_have_the_exact_public_values() {
    for (actual, expected) in [
        (theme::CANVAS, 0x201D1D),
        (theme::SURFACE, 0x302C2C),
        (theme::SURFACE_ELEVATED, 0x3A3636),
        (theme::TERMINAL_BG, 0x161313),
        (theme::INK, 0xFDFCFC),
        (theme::BODY, 0xD8D5D5),
        (theme::MUTE, 0x9A9898),
        (theme::STONE, 0x6E6E73),
        (theme::HAIRLINE_RGBA, 0xFDFCFC1F),
        (theme::HAIRLINE_STRONG, 0x646262),
        (theme::ACCENT, 0x0A84FF),
        (theme::SUCCESS, 0x30D158),
        (theme::WARNING, 0xFF9F0A),
        (theme::DANGER, 0xFF453A),
        (theme::CLAUDE, 0xDA8FFF),
        (theme::CODEX, 0x5AC8FA),
    ] {
        let actual: u32 = actual;
        assert_eq!(actual, expected);
    }
    let font: &'static str = theme::FONT_FAMILY;
    assert_eq!(font, "JetBrains Mono");
}

#[test]
fn every_state_has_its_exact_marker_spanish_label_and_color() {
    for (state, expected) in [
        (SessionState::Working, ("[>]", "trabajando", theme::SUCCESS)),
        (
            SessionState::NeedsYou,
            ("[?]", "te necesita", theme::WARNING),
        ),
        (SessionState::Waiting, ("[+]", "en espera", theme::MUTE)),
        (
            SessionState::Finished { failed: false },
            ("[x]", "terminada", theme::MUTE),
        ),
        (
            SessionState::Finished { failed: true },
            ("[x]", "terminada con error", theme::DANGER),
        ),
    ] {
        let actual: (&'static str, &'static str, u32) = state_style(state);
        assert_eq!(actual, expected, "{state:?}");
    }
}

#[test]
fn agent_colors_pills_and_lines_cover_both_agents_with_and_without_models() {
    for (cli, tag, label, color) in [
        (AgentCli::ClaudeCode, "cl", "Claude Code", theme::CLAUDE),
        (AgentCli::Codex, "cx", "Codex", theme::CODEX),
    ] {
        assert_eq!(agent_color(cli), color);
        let mut s = session(cli, "one", "worker");
        assert_eq!(pill(&s), (tag, String::new()));
        assert_eq!(agent_line(&s), label);
        s.model = Some("nebula".into());
        assert_eq!(pill(&s), (tag, "nebula".into()));
        assert_eq!(agent_line(&s), format!("{label} · nebula"));
    }
}

#[test]
fn attach_line_uses_option_presence_and_joins_arguments_with_spaces() {
    for cli in [AgentCli::ClaudeCode, AgentCli::Codex] {
        let mut s = session(cli, "one", "worker");
        assert_eq!(attach_line(&s), None);
        s.attach = Some(vec![
            "fictional-cli".into(),
            "resume".into(),
            "job-a".into(),
        ]);
        assert_eq!(attach_line(&s), Some("$ fictional-cli resume job-a".into()));
        s.attach = Some(vec!["fictional-cli".into(), "two words".into()]);
        assert_eq!(attach_line(&s), Some("$ fictional-cli two words".into()));
        s.attach = Some(vec![]);
        assert_eq!(attach_line(&s), Some("$ ".into()));
    }
}

#[test]
fn header_counts_keep_fixed_order_colors_and_zero_counts() {
    assert_eq!(
        header_counts(&[]),
        [
            ("[>]", "0 trabajando".into(), theme::SUCCESS),
            ("[?]", "0 te necesita".into(), theme::WARNING),
            ("[+]", "0 en espera".into(), theme::MUTE),
        ]
    );
    let states = [
        SessionState::Waiting,
        SessionState::Working,
        SessionState::Finished { failed: false },
        SessionState::NeedsYou,
        SessionState::Waiting,
        SessionState::Working,
        SessionState::Finished { failed: true },
        SessionState::Waiting,
    ];
    let sessions: Vec<_> = states
        .into_iter()
        .enumerate()
        .map(|(i, state)| {
            let mut s = session(AgentCli::Codex, &format!("job-{i}"), "worker");
            s.state = state;
            if i % 2 == 0 {
                s.project.root = "/fictional/other".into();
                s.key.cli = AgentCli::ClaudeCode;
            }
            s
        })
        .collect();
    assert_eq!(
        header_counts(&sessions),
        [
            ("[>]", "2 trabajando".into(), theme::SUCCESS),
            ("[?]", "1 te necesita".into(), theme::WARNING),
            ("[+]", "3 en espera".into(), theme::MUTE),
        ]
    );
    assert_eq!(header_counts(&sessions[2..3]), header_counts(&[]));
    assert_eq!(header_counts(&sessions[6..7]), header_counts(&[]));
}

#[test]
fn detail_requires_the_full_session_key() {
    let s = session(AgentCli::ClaudeCode, "shared-id", "worker");
    assert_eq!(detail(&[], &s.key, 0, None), None);
    let absent = SessionKey {
        cli: AgentCli::Codex,
        id: s.key.id.clone(),
    };
    assert_eq!(detail(&[s], &absent, 0, None), None);
}

#[test]
fn standalone_detail_shows_project_folder_unknown_time_and_no_talks() {
    let s = session(AgentCli::Codex, "one", "worker");
    let sessions = [s.clone()];
    assert_eq!(
        detail(&sessions, &s.key, 300_000, Some("/srv/fictional")),
        Some(Detail {
            project: "Orchid".into(),
            role: "sesión suelta",
            launched_by: "tú".into(),
            since: "—".into(),
            folder: "~/orchid/branch".into(),
            talks: vec![],
        })
    );
    for home in [None, Some("/fictional/elsewhere")] {
        assert_eq!(detail(&sessions, &s.key, 0, home).unwrap().folder, s.cwd);
    }
}

#[test]
fn detail_uses_state_time_instead_of_start_time() {
    let mut s = session(AgentCli::Codex, "one", "worker");
    s.started_at_ms = Some(0);
    for (since, label) in [
        (180_000_000, "ahora"),
        (179_880_000, "hace 2 min"),
        (172_800_000, "hace 2 h"),
        (7_200_000, "hace 2 d"),
    ] {
        s.state_since_ms = Some(since);
        assert_eq!(
            detail(&[s.clone()], &s.key, 180_000_000, None)
                .unwrap()
                .since,
            label
        );
    }
}

#[test]
fn missing_parent_still_makes_an_assigned_session_without_a_parent_talk() {
    let mut s = session(AgentCli::Codex, "one", "worker");
    s.parent = Some(SessionKey {
        cli: AgentCli::ClaudeCode,
        id: "absent".into(),
    });
    let panel = detail(&[s.clone()], &s.key, 0, None).unwrap();
    assert_eq!(panel.role, "encargada");
    assert_eq!(panel.launched_by, "tú");
    assert!(panel.talks.is_empty());
}

#[test]
fn existing_parent_uses_its_model_or_tag_and_exact_talk_state() {
    for (cli, fallback, color) in [
        (AgentCli::ClaudeCode, "cl", theme::CLAUDE),
        (AgentCli::Codex, "cx", theme::CODEX),
    ] {
        let mut parent = session(cli, "parent", "planner");
        parent.state = SessionState::NeedsYou;
        let mut child = session(AgentCli::Codex, "child", "worker");
        child.parent = Some(parent.key.clone());
        for model in [None, Some("nebula")] {
            parent.model = model.map(str::to_owned);
            let name = model.unwrap_or(fallback);
            let panel = detail(&[child.clone(), parent.clone()], &child.key, 0, None).unwrap();
            assert_eq!(panel.role, "encargada");
            assert_eq!(panel.launched_by, format!("planner ({name})"));
            assert_eq!(
                panel.talks,
                vec![talk(
                    "lanzada por",
                    &format!("{name} · planner"),
                    color,
                    "[?] te necesita"
                )]
            );
        }
    }
}

#[test]
fn talks_put_parent_first_then_direct_children_by_start_and_name() {
    let mut parent = session(AgentCli::ClaudeCode, "parent", "planner");
    parent.model = Some("nebula".into());
    parent.state = SessionState::Waiting;
    let mut selected = session(AgentCli::Codex, "selected", "coordinator");
    selected.parent = Some(parent.key.clone());
    let mut children = Vec::new();
    for (id, name, start, cli, model, state) in [
        (
            "early",
            "zeta",
            Some(-1),
            AgentCli::Codex,
            None,
            SessionState::Working,
        ),
        (
            "alpha",
            "alpha",
            Some(20),
            AgentCli::ClaudeCode,
            Some("nova"),
            SessionState::Finished { failed: false },
        ),
        (
            "beta",
            "beta",
            Some(20),
            AgentCli::Codex,
            Some("luna"),
            SessionState::Finished { failed: true },
        ),
        (
            "unknown-a",
            "aardvark",
            None,
            AgentCli::ClaudeCode,
            None,
            SessionState::NeedsYou,
        ),
        (
            "unknown-z",
            "zulu",
            None,
            AgentCli::Codex,
            None,
            SessionState::Waiting,
        ),
    ] {
        let mut child = session(cli, id, name);
        child.parent = Some(selected.key.clone());
        child.started_at_ms = start;
        child.model = model.map(str::to_owned);
        child.state = state;
        children.push(child);
    }
    let mut grandchild = session(AgentCli::Codex, "grandchild", "nested");
    grandchild.parent = Some(children[0].key.clone());
    let mut wrong_cli_child = session(AgentCli::Codex, "unrelated", "unrelated");
    wrong_cli_child.parent = Some(SessionKey {
        cli: AgentCli::ClaudeCode,
        id: selected.key.id.clone(),
    });
    let sessions = [
        children[4].clone(),
        children[2].clone(),
        grandchild,
        children[1].clone(),
        selected.clone(),
        children[3].clone(),
        parent,
        children[0].clone(),
        wrong_cli_child,
    ];
    let panel = detail(&sessions, &selected.key, 0, None).unwrap();
    assert_eq!(panel.role, "director");
    assert_eq!(panel.launched_by, "planner (nebula)");
    assert_eq!(
        panel.talks,
        vec![
            talk(
                "lanzada por",
                "nebula · planner",
                theme::CLAUDE,
                "[+] en espera"
            ),
            talk("lanzó a", "cx · zeta", theme::CODEX, "[>] trabajando"),
            talk("lanzó a", "nova · alpha", theme::CLAUDE, "[x] terminada"),
            talk(
                "lanzó a",
                "luna · beta",
                theme::CODEX,
                "[x] terminada con error"
            ),
            talk("lanzó a", "cl · aardvark", theme::CLAUDE, "[?] te necesita"),
            talk("lanzó a", "cx · zulu", theme::CODEX, "[+] en espera"),
        ]
    );
    let root = &sessions[6];
    let root_panel = detail(&sessions, &root.key, 0, None).unwrap();
    assert_eq!(root_panel.role, "director");
    assert_eq!(root_panel.launched_by, "tú");
    assert_eq!(
        root_panel.talks,
        vec![talk(
            "lanzó a",
            "cx · coordinator",
            theme::CODEX,
            "[>] trabajando"
        )]
    );
}

#[test]
fn stop_action_covers_agents_live_states_attachability_and_terminal_presence() {
    for cli in [AgentCli::ClaudeCode, AgentCli::Codex] {
        for state in [
            SessionState::Working,
            SessionState::NeedsYou,
            SessionState::Waiting,
            SessionState::Finished { failed: false },
            SessionState::Finished { failed: true },
        ] {
            for attachable in [false, true] {
                let mut s = session(cli, "one", "worker");
                s.state = state;
                if attachable {
                    s.kind = SessionKind::Background;
                    s.attach = Some(vec!["fictional-cli".into(), "attach".into()]);
                }
                for terminal_open in [false, true] {
                    let expected = match (state, cli, attachable, terminal_open) {
                        (SessionState::Finished { .. }, _, _, _) => StopAction::Disabled,
                        (_, AgentCli::ClaudeCode, true, _) => StopAction::StopSession,
                        (_, AgentCli::Codex, _, true) => StopAction::CloseTerminal,
                        _ => StopAction::Disabled,
                    };
                    assert_eq!(
                        stop_action(&s, terminal_open),
                        expected,
                        "{cli:?} {state:?} attach={attachable} terminal={terminal_open}"
                    );
                }
            }
        }
    }
}

#[test]
fn grid_size_subtracts_both_paddings_floors_and_clamps_each_axis() {
    for (input, expected) in [
        ((100.0, 80.0, 10.0, 8.0, 12.0), (10, 5)),
        ((103.0, 83.0, 10.0, 8.0, 12.0), (10, 5)),
        ((10.0, 80.0, 10.0, 8.0, 12.0), (1, 5)),
        ((100.0, 10.0, 10.0, 8.0, 12.0), (10, 1)),
        ((0.5, 0.5, 0.0, 1.0, 1.0), (1, 1)),
        ((1000.0, 1001.0, 0.0, 1.0, 1.0), (1000, 1000)),
        ((2000.0, 80.0, 0.0, 1.0, 10.0), (1000, 8)),
        ((100.0, 2000.0, 0.0, 10.0, 1.0), (10, 1000)),
    ] {
        let (w, h, p, cw, lh) = input;
        let actual: (u16, u16) = grid_size(w, h, p, cw, lh);
        assert_eq!(actual, expected, "{input:?}");
    }
}

#[test]
fn grid_size_always_stays_in_bounds_for_unusual_inputs() {
    // The contract specifies bounds, but no exact fallback for invalid inputs.
    for value in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.0,
        -0.0,
        -1.0,
        f32::MIN,
        f32::MAX,
        f32::MIN_POSITIVE,
    ] {
        for index in 0..5 {
            let mut input = [100.0, 80.0, 10.0, 8.0, 12.0];
            input[index] = value;
            let [w, h, p, cw, lh] = input;
            let (cols, rows) = grid_size(w, h, p, cw, lh);
            assert!((1..=1000).contains(&cols), "columns for {input:?}: {cols}");
            assert!((1..=1000).contains(&rows), "rows for {input:?}: {rows}");
        }
        let (cols, rows) = grid_size(value, value, value, value, value);
        assert!((1..=1000).contains(&cols));
        assert!((1..=1000).contains(&rows));
    }
}

#[test]
fn non_attachable_messages_are_exact_for_each_agent() {
    assert_eq!(
        not_attachable_text(AgentCli::ClaudeCode),
        (
            "Sesión interactiva de Claude Code",
            "Está abierta en otro terminal. titan-agent solo puede adjuntarse a sesiones en segundo plano (claude --bg)."
        )
    );
    assert_eq!(
        not_attachable_text(AgentCli::Codex),
        (
            "Sesión de Codex sin terminal",
            "Codex no ofrece un comando para adjuntarse a esta sesión."
        )
    );
}

#[test]
fn monitor_without_a_team_includes_only_project_sessions_in_input_order() {
    let p = project();
    let empty = (
        "Sin equipo trabajando en Orchid".into(),
        "El monitor se anima cuando las sesiones del proyecto se encargan trabajo entre ellas.",
        "Sesiones sueltas: ".into(),
    );
    assert_eq!(monitor_text(&[], &p), empty);
    let mut first = session(AgentCli::Codex, "first", "zeta");
    first.parent = Some(SessionKey {
        cli: AgentCli::ClaudeCode,
        id: "absent".into(),
    });
    let second = session(AgentCli::ClaudeCode, "second", "alpha");
    let mut elsewhere = session(AgentCli::ClaudeCode, "elsewhere", "excluded");
    elsewhere.project.root = "/fictional/another-orchid".into();
    let mut other_name = session(AgentCli::Codex, "other-name", "also excluded");
    other_name.project.name = "Zinnia".into();
    assert_eq!(
        monitor_text(&[elsewhere.clone(), other_name.clone()], &p),
        empty
    );
    assert_eq!(
        monitor_text(&[first, elsewhere, second, other_name], &p),
        (
            "Sin equipo trabajando en Orchid".into(),
            "El monitor se anima cuando las sesiones del proyecto se encargan trabajo entre ellas.",
            "Sesiones sueltas: zeta, alpha".into(),
        )
    );
}

#[test]
fn monitor_requires_a_parent_in_the_same_project_and_does_not_require_live_states() {
    let p = project();
    let mut parent = session(AgentCli::ClaudeCode, "parent", "planner");
    let mut child = session(AgentCli::Codex, "child", "worker");
    child.parent = Some(parent.key.clone());
    parent.project.root = "/fictional/elsewhere".into();
    assert_eq!(
        monitor_text(&[child.clone(), parent.clone()], &p),
        (
            "Sin equipo trabajando en Orchid".into(),
            "El monitor se anima cuando las sesiones del proyecto se encargan trabajo entre ellas.",
            "Sesiones sueltas: worker".into(),
        )
    );
    parent.project = p.clone();
    for state in [
        SessionState::Working,
        SessionState::NeedsYou,
        SessionState::Waiting,
        SessionState::Finished { failed: false },
        SessionState::Finished { failed: true },
    ] {
        parent.state = state;
        child.state = state;
        assert_eq!(
            monitor_text(&[child.clone(), parent.clone()], &p),
            (
                "Equipo trabajando en Orchid".into(),
                "El monitor en tiempo real llega en la fase 3.",
                "Sesiones: worker, planner".into(),
            )
        );
    }
}
