//! Tokens inherited from titan-ssh (docs/Decisiones visuales/Lenguaje visual
//! heredado de titan-ssh.md). Nothing else in the interface picks colours.

use gpui::{Hsla, Rgba, rgb, rgba};
use titan_agent_core::session::{AgentCli, SessionState};

pub const FONT: &str = "JetBrains Mono";

pub const CANVAS: u32 = 0x201d1d;
pub const SURFACE: u32 = 0x302c2c;
pub const SURFACE_ELEVATED: u32 = 0x3a3636;
pub const TERMINAL_BG: u32 = 0x161313;
pub const INK: u32 = 0xfdfcfc;
pub const BODY: u32 = 0xd8d5d5;
pub const MUTE: u32 = 0x9a9898;
pub const STONE: u32 = 0x6e6e73;
pub const HAIRLINE_STRONG: u32 = 0x646262;
pub const ACCENT: u32 = 0x0a84ff;
pub const SUCCESS: u32 = 0x30d158;
pub const WARNING: u32 = 0xff9f0a;
pub const DANGER: u32 = 0xff453a;
pub const CLAUDE: u32 = 0xda8fff;
pub const CODEX: u32 = 0x5ac8fa;

/// `hairline`: rgba(253,252,252,0.12).
pub fn hairline() -> Rgba {
    rgba(0xfdfcfc1f)
}

pub fn color(hex: u32) -> Rgba {
    rgb(hex)
}

pub fn hsla(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn agent_color(agent: AgentCli) -> u32 {
    match agent {
        AgentCli::ClaudeCode => CLAUDE,
        AgentCli::Codex => CODEX,
    }
}

pub fn state_color(state: SessionState) -> u32 {
    match state {
        SessionState::Working => SUCCESS,
        SessionState::NeedsYou => WARNING,
        SessionState::Waiting => MUTE,
        SessionState::Finished { failed: true } => DANGER,
        SessionState::Finished { failed: false } => MUTE,
    }
}

/// ANSI palette of the titan-ssh terminal (indices 0 to 15).
pub const ANSI: [u32; 16] = [
    0x201d1d, 0xff453a, 0x30d158, 0xff9f0a, 0x0a84ff, 0xbf5af2, 0x5ac8fa, 0xd8d5d5, 0x6e6e73,
    0xff6961, 0x66d97e, 0xffb340, 0x409cff, 0xda8fff, 0x8fe0ff, 0xfdfcfc,
];

/// Resolves an xterm 256-colour index.
pub fn xterm(index: u8) -> u32 {
    match index {
        0..=15 => ANSI[index as usize],
        16..=231 => {
            let i = index - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v as u32 };
            (level(i / 36) << 16) | (level((i / 6) % 6) << 8) | level(i % 6)
        }
        _ => {
            let gray = 8 + 10 * (index as u32 - 232);
            (gray << 16) | (gray << 8) | gray
        }
    }
}
