use crate::theme;
use alacritty_terminal::{
    event::VoidListener,
    index::{Column, Line},
    term::{
        Config, Term, TermMode,
        cell::{Cell, Flags},
        test::TermSize,
    },
    vte::ansi::{Color, NamedColor, Processor, Rgb},
};
use titan_agent_core::keys::KeyModes;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub fg: u32,
    pub bg: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledLine {
    pub spans: Vec<Span>,
}
pub struct TerminalModel {
    term: Term<VoidListener>,
    parser: Processor,
    size: (u16, u16),
}
impl TerminalModel {
    pub fn new(cols: u16, rows: u16) -> Self {
        let size = TermSize::new(usize::from(cols.max(2)), usize::from(rows.max(1)));
        Self {
            term: Term::new(Config::default(), &size, VoidListener),
            parser: Processor::new(),
            size: (cols.max(1), rows.max(1)),
        }
    }
    pub fn feed(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.parser
                .advance(&mut self.term, std::slice::from_ref(byte))
        }
    }
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.size = (cols.max(1), rows.max(1));
        self.term.resize(TermSize::new(
            self.size.0.max(2) as usize,
            self.size.1 as usize,
        ));
    }
    pub fn size(&self) -> (u16, u16) {
        self.size
    }
    pub fn cursor(&self) -> Option<(u16, u16)> {
        if !self.term.mode().contains(TermMode::SHOW_CURSOR) {
            return None;
        }
        let p = self.term.grid().cursor.point;
        Some((p.line.0.max(0) as u16, p.column.0 as u16))
    }
    pub fn key_modes(&self) -> KeyModes {
        KeyModes {
            app_cursor: self.term.mode().contains(TermMode::APP_CURSOR),
            bracketed_paste: self.term.mode().contains(TermMode::BRACKETED_PASTE),
        }
    }
    pub fn lines(&self) -> Vec<StyledLine> {
        let mut result = Vec::new();
        for row in 0..self.size.1 {
            let mut spans: Vec<Span> = Vec::new();
            let mut col = 0;
            while col < usize::from(self.size.0) {
                let cell = &self.term.grid()[Line(row as i32)][Column(col)];
                let flags = cell.flags;
                let (mut fg, mut bg) = colors(cell);
                let bold = flags.contains(Flags::BOLD);
                if bold && let Color::Named(n) = cell.fg {
                    let index = n as usize;
                    if index < 8 {
                        fg = theme::ANSI[index + 8];
                    }
                }
                if flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg)
                }
                if flags.contains(Flags::DIM) {
                    fg = mix(fg, bg)
                }
                if flags.contains(Flags::HIDDEN) {
                    fg = bg
                }
                let style = (
                    fg,
                    bg,
                    bold,
                    flags.contains(Flags::ITALIC),
                    flags.intersects(Flags::ALL_UNDERLINES),
                    flags.contains(Flags::STRIKEOUT),
                );
                if flags.contains(Flags::WIDE_CHAR_SPACER) {
                    col += 1;
                    continue;
                }
                let mut text = String::new();
                text.push(cell.c);
                if let Some(z) = cell.zerowidth() {
                    text.extend(z)
                }
                let width = if flags.contains(Flags::WIDE_CHAR) {
                    2
                } else {
                    1
                };
                if let Some(last) = spans
                    .last_mut()
                    .filter(|s| (s.fg, s.bg, s.bold, s.italic, s.underline, s.strikeout) == style)
                {
                    last.text.push_str(&text)
                } else {
                    spans.push(Span {
                        text,
                        fg,
                        bg,
                        bold,
                        italic: style.3,
                        underline: style.4,
                        strikeout: style.5,
                    })
                }
                col += width;
            }
            result.push(StyledLine { spans });
        }
        result
    }
}
fn colors(cell: &Cell) -> (u32, u32) {
    (color(cell.fg), color(cell.bg))
}
fn color(c: Color) -> u32 {
    match c {
        Color::Indexed(i) => theme::indexed_color(i),
        Color::Spec(Rgb { r, g, b }) => (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
        Color::Named(n) => match n {
            NamedColor::Foreground => theme::TERMINAL_FG,
            NamedColor::Background => theme::TERMINAL_BG,
            NamedColor::Cursor => theme::TERMINAL_CURSOR,
            NamedColor::DimBlack => theme::ANSI[0],
            NamedColor::DimRed => theme::ANSI[1],
            NamedColor::DimGreen => theme::ANSI[2],
            NamedColor::DimYellow => theme::ANSI[3],
            NamedColor::DimBlue => theme::ANSI[4],
            NamedColor::DimMagenta => theme::ANSI[5],
            NamedColor::DimCyan => theme::ANSI[6],
            NamedColor::DimWhite => theme::ANSI[7],
            _ => theme::ANSI[(n as usize).min(15)],
        },
    }
}
fn mix(fg: u32, bg: u32) -> u32 {
    let ch = |shift: u32| ((2 * ((fg >> shift) & 255) + ((bg >> shift) & 255)) / 3) & 255;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}
