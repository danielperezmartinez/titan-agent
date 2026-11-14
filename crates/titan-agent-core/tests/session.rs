use std::fmt::Debug;
use std::hash::Hash;

use titan_agent_core::session::{AgentCli, SessionState, StateCounts};

#[test]
fn agent_clis_have_the_specified_order_and_labels() {
    const ALL: [AgentCli; 2] = AgentCli::ALL;
    assert_eq!(ALL, [AgentCli::ClaudeCode, AgentCli::Codex]);

    for (cli, command, tag, display) in [
        (AgentCli::ClaudeCode, "claude", "cl", "Claude Code"),
        (AgentCli::Codex, "codex", "cx", "Codex"),
    ] {
        let actual_command: &'static str = cli.command();
        let actual_tag: &'static str = cli.tag();
        assert_eq!(actual_command, command);
        assert_eq!(actual_tag, tag);
        assert_eq!(cli.to_string(), display);
        assert_eq!(AgentCli::from_command(command), Some(cli));
    }
}

#[test]
fn commands_require_an_exact_executable_name() {
    for command in [
        "",
        "Claude",
        "Claude Code",
        "CLAUDE",
        "Codex",
        "CODEX",
        " claude",
        "claude ",
        " codex",
        "codex ",
        "\tcodex",
        "claude\n",
        "/example/bin/claude",
        "/example/bin/codex",
        "./codex",
        "codex.exe",
        "claude --help",
        "cl",
        "cx",
        "other-cli",
    ] {
        assert_eq!(AgentCli::from_command(command), None, "{command:?}");
    }
}

#[test]
fn every_state_has_its_ascii_marker_and_liveness() {
    for (state, marker, live) in [
        (SessionState::Working, "[>]", true),
        (SessionState::NeedsYou, "[?]", true),
        (SessionState::Waiting, "[+]", true),
        (SessionState::Finished { failed: false }, "[x]", false),
        (SessionState::Finished { failed: true }, "[x]", false),
    ] {
        let actual_marker: &'static str = state.marker();
        assert_eq!(actual_marker, marker, "{state:?}");
        assert!(actual_marker.is_ascii());
        assert_eq!(state.is_live(), live, "{state:?}");
    }
}

#[test]
fn empty_input_and_default_counts_are_zero() {
    let expected = StateCounts {
        working: 0,
        needs_you: 0,
        waiting: 0,
    };
    assert_eq!(StateCounts::default(), expected);
    assert_eq!(StateCounts::tally([]), expected);
    assert_eq!(StateCounts::tally(std::iter::empty()), expected);
    assert_eq!(expected.total(), 0);
}

#[test]
fn each_live_state_increments_only_its_own_counter() {
    for (state, expected) in [
        (
            SessionState::Working,
            StateCounts {
                working: 1,
                needs_you: 0,
                waiting: 0,
            },
        ),
        (
            SessionState::NeedsYou,
            StateCounts {
                working: 0,
                needs_you: 1,
                waiting: 0,
            },
        ),
        (
            SessionState::Waiting,
            StateCounts {
                working: 0,
                needs_you: 0,
                waiting: 1,
            },
        ),
    ] {
        let counts = StateCounts::tally([state]);
        assert_eq!(counts, expected, "{state:?}");
        assert_eq!(counts.total(), 1);
    }
}

#[test]
fn finished_sessions_never_contribute_to_counts() {
    for failed in [false, true] {
        let counts = StateCounts::tally([SessionState::Finished { failed }; 3]);
        assert_eq!(counts, StateCounts::default());
        assert_eq!(counts.total(), 0);
    }
}

#[test]
fn mixed_states_count_repetitions_and_ignore_finished_sessions() {
    use SessionState::{Finished, NeedsYou, Waiting, Working};

    let states = [
        Working,
        Finished { failed: false },
        Waiting,
        NeedsYou,
        Working,
        Waiting,
        Finished { failed: true },
        Waiting,
    ];
    let expected = StateCounts {
        working: 2,
        needs_you: 1,
        waiting: 3,
    };
    assert_eq!(StateCounts::tally(states), expected);
    assert_eq!(StateCounts::tally(states.into_iter().rev()), expected);
    assert_eq!(expected.total(), 6);
}

#[test]
fn total_sums_public_fields_without_changing_them() {
    let counts = StateCounts {
        working: 4,
        needs_you: 2,
        waiting: 7,
    };
    assert_eq!(counts.total(), 13);
    assert_eq!(counts.working, 4);
    assert_eq!(counts.needs_you, 2);
    assert_eq!(counts.waiting, 7);
}

#[test]
fn public_types_support_the_required_traits() {
    fn assert_value_traits<T: Debug + Clone + Copy + PartialEq + Eq>() {}
    fn assert_hash<T: Hash>() {}
    fn assert_default<T: Default>() {}

    assert_value_traits::<AgentCli>();
    assert_value_traits::<SessionState>();
    assert_value_traits::<StateCounts>();
    assert_hash::<AgentCli>();
    assert_hash::<SessionState>();
    assert_default::<StateCounts>();

    assert_ne!(AgentCli::ClaudeCode, AgentCli::Codex);
    assert_ne!(
        SessionState::Finished { failed: false },
        SessionState::Finished { failed: true },
    );
}
