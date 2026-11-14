use titan_agent::theme::*;
use titan_agent_core::session::{AgentCli, SessionState};

#[test]
fn design_tokens_match_the_specification() {
    let tokens: &[(u32, u32)] = &[
        (CANVAS, 0x201d1d),
        (INK, 0xfdfcfc),
        (SURFACE, 0x302c2c),
        (BODY, 0xd8d5d5),
        (SURFACE_ELEVATED, 0x3a3636),
        (MUTE, 0x9a9898),
        (TERMINAL_BG, 0x161313),
        (STONE, 0x6e6e73),
        (HAIRLINE_STRONG, 0x646262),
        (ACCENT, 0x0a84ff),
        (SUCCESS, 0x30d158),
        (WARNING, 0xff9f0a),
        (DANGER, 0xff453a),
        (CLAUDE, 0xda8fff),
        (CODEX, 0x5ac8fa),
        (TERMINAL_FG, 0xd8d5d5),
        (TERMINAL_CURSOR, 0x0a84ff),
        (HAIRLINE_RGBA, 0xfdfcfc1f),
    ];
    for &(actual, expected) in tokens {
        assert_eq!(actual, expected);
    }
}

#[test]
fn ansi_palette_and_indexed_colors_match_all_256_entries() {
    let expected: [u32; 16] = [
        0x201d1d, 0xff453a, 0x30d158, 0xff9f0a, 0x0a84ff, 0xbf5af2, 0x5ac8fa, 0xd8d5d5, 0x6e6e73,
        0xff6961, 0x66d97e, 0xffb340, 0x409cff, 0xda8fff, 0x8fe0ff, 0xfdfcfc,
    ];
    assert_eq!(ANSI, expected);
    for (index, color) in expected.into_iter().enumerate() {
        assert_eq!(indexed_color(index as u8), color);
    }
    let levels = [0_u32, 95, 135, 175, 215, 255];
    for r in 0..6 {
        for g in 0..6 {
            for b in 0..6 {
                let index = (16 + 36 * r + 6 * g + b) as u8;
                let expected = (levels[r] << 16) | (levels[g] << 8) | levels[b];
                assert_eq!(indexed_color(index), expected, "index {index}");
            }
        }
    }
    for index in 232_u8..=255 {
        let level = 8 + 10 * u32::from(index - 232);
        assert_eq!(indexed_color(index), level * 0x010101, "index {index}");
    }
}

#[test]
fn state_and_agent_colors_match_every_variant() {
    for (state, expected) in [
        (SessionState::Working, SUCCESS),
        (SessionState::NeedsYou, WARNING),
        (SessionState::Waiting, MUTE),
        (SessionState::Finished { failed: false }, MUTE),
        (SessionState::Finished { failed: true }, DANGER),
    ] {
        assert_eq!(state_color(state), expected);
    }
    assert_eq!(agent_color(AgentCli::ClaudeCode), CLAUDE);
    assert_eq!(agent_color(AgentCli::Codex), CODEX);
}
