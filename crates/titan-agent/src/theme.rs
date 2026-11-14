use titan_agent_core::session::{AgentCli, SessionState};
pub const CANVAS: u32 = 0x201d1d;
pub const INK: u32 = 0xfdfcfc;
pub const SURFACE: u32 = 0x302c2c;
pub const BODY: u32 = 0xd8d5d5;
pub const SURFACE_ELEVATED: u32 = 0x3a3636;
pub const MUTE: u32 = 0x9a9898;
pub const TERMINAL_BG: u32 = 0x161313;
pub const STONE: u32 = 0x6e6e73;
pub const HAIRLINE_STRONG: u32 = 0x646262;
pub const ACCENT: u32 = 0x0a84ff;
pub const SUCCESS: u32 = 0x30d158;
pub const WARNING: u32 = 0xff9f0a;
pub const DANGER: u32 = 0xff453a;
pub const CLAUDE: u32 = 0xda8fff;
pub const CODEX: u32 = 0x5ac8fa;
pub const TERMINAL_FG: u32 = BODY;
pub const TERMINAL_CURSOR: u32 = ACCENT;
pub const HAIRLINE_RGBA: u32 = 0xfdfcfc1f;
pub const ANSI: [u32; 16] = [
    0x201d1d, 0xff453a, 0x30d158, 0xff9f0a, 0x0a84ff, 0xbf5af2, 0x5ac8fa, 0xd8d5d5, 0x6e6e73,
    0xff6961, 0x66d97e, 0xffb340, 0x409cff, 0xda8fff, 0x8fe0ff, 0xfdfcfc,
];
pub fn indexed_color(i: u8) -> u32 {
    if i < 16 {
        return ANSI[i as usize];
    }
    if i < 232 {
        let n = i - 16;
        let f = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
        let (r, g, b) = (n / 36, (n / 6) % 6, n % 6);
        (u32::from(f(r)) << 16) | (u32::from(f(g)) << 8) | u32::from(f(b))
    } else {
        let v = 8 + 10 * (i - 232);
        u32::from(v) * 0x010101
    }
}
pub fn state_color(s: SessionState) -> u32 {
    match s {
        SessionState::Working => SUCCESS,
        SessionState::NeedsYou => WARNING,
        SessionState::Waiting | SessionState::Finished { failed: false } => MUTE,
        SessionState::Finished { failed: true } => DANGER,
    }
}
pub fn agent_color(c: AgentCli) -> u32 {
    match c {
        AgentCli::ClaudeCode => CLAUDE,
        AgentCli::Codex => CODEX,
    }
}
