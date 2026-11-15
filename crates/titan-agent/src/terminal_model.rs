//! Client-side terminal model rendered by the window.

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line as GridLine};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{self, Color, NamedColor};

pub mod palette {
    /// Default foreground (`body`).
    pub const DEFAULT_FG: u32 = 0xD8D5D5;
    /// Default background, the terminal well (`terminal-bg`).
    pub const DEFAULT_BG: u32 = 0x161313;
    /// Cursor (`accent`).
    pub const CURSOR: u32 = 0x0A84FF;
    /// ANSI 0-15 colors.
    pub const ANSI: [u32; 16] = [
        0x201D1D, 0xFF453A, 0x30D158, 0xFF9F0A, 0x0A84FF, 0xBF5AF2, 0x5AC8FA, 0xD8D5D5, 0x6E6E73,
        0xFF6961, 0x66D97E, 0xFFB340, 0x409CFF, 0xDA8FFF, 0x8FE0FF, 0xFDFCFC,
    ];

    /// Resolve an xterm 256-colour palette entry.
    pub fn indexed(index: u8) -> u32 {
        match index {
            0..=15 => ANSI[usize::from(index)],
            16..=231 => {
                let levels = [0, 95, 135, 175, 215, 255];
                let value = usize::from(index - 16);
                ((levels[value / 36] as u32) << 16)
                    | ((levels[(value / 6) % 6] as u32) << 8)
                    | levels[value % 6] as u32
            }
            232..=255 => u32::from(8 + 10 * (index - 232)) * 0x010101,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modes {
    pub app_cursor: bool,
    pub bracketed_paste: bool,
    pub alt_screen: bool,
    pub mouse_reporting: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub cols: u16,
    pub rows: u16,
    pub lines: Vec<Line>,
    pub cursor: Option<Cursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    pub runs: Vec<Run>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub line: u16,
    pub col: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub col: u16,
    pub cells: u16,
    pub text: String,
    pub fg: u32,
    pub bg: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

struct SilentListener;
impl EventListener for SilentListener {
    fn send_event(&self, _event: Event) {}
}

pub struct TerminalModel {
    term: Term<SilentListener>,
    processor: ansi::Processor,
    cols: u16,
    rows: u16,
}

impl TerminalModel {
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let size = alacritty_terminal::term::test::TermSize::new(cols.into(), rows.into());
        Self {
            term: Term::new(Config::default(), &size, SilentListener),
            processor: ansi::Processor::new(),
            cols,
            rows,
        }
    }

    pub fn load_snapshot(&mut self, cols: u16, rows: u16, snapshot: &[u8]) {
        *self = Self::new(cols, rows);
        self.feed(snapshot);
        self.scroll_to_bottom();
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        // Alacritty interprets SGR 21 as “bold off”; this model follows the
        // terminal convention where it enables double underline.
        let rewritten = if bytes.windows(5).any(|window| window == b"\x1b[21m") {
            let mut output = Vec::with_capacity(bytes.len());
            let mut rest = bytes;
            while let Some(index) = rest.windows(5).position(|window| window == b"\x1b[21m") {
                output.extend_from_slice(&rest[..index]);
                output.extend_from_slice(b"\x1b[4:2m");
                rest = &rest[index + 5..];
            }
            output.extend_from_slice(rest);
            Some(output)
        } else {
            None
        };
        self.processor
            .advance(&mut self.term, rewritten.as_deref().unwrap_or(bytes));
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols.max(1);
        self.rows = rows.max(1);
        self.term
            .resize(alacritty_terminal::term::test::TermSize::new(
                self.cols.into(),
                self.rows.into(),
            ));
    }

    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    pub fn scroll(&mut self, lines: i32) {
        let offset = self.display_offset() as i64;
        let target = (offset + i64::from(lines)).clamp(0, self.term.grid().history_size() as i64);
        self.term
            .scroll_display(Scroll::Delta((target - offset) as i32));
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    pub fn modes(&self) -> Modes {
        let mode = self.term.mode();
        Modes {
            app_cursor: mode.contains(TermMode::APP_CURSOR),
            bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
            alt_screen: mode.contains(TermMode::ALT_SCREEN),
            mouse_reporting: mode.intersects(TermMode::MOUSE_MODE),
        }
    }

    pub fn frame(&self) -> Frame {
        let offset = self.display_offset();
        let grid = self.term.grid();
        let mut lines = Vec::with_capacity(usize::from(self.rows));

        for visible_line in 0..self.rows {
            let grid_line = GridLine(-(offset as i32) + i32::from(visible_line));
            let mut cells = Vec::with_capacity(usize::from(self.cols));
            for col in 0..self.cols {
                let cell = &grid[grid_line][Column(usize::from(col))];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let style = style(cell);
                let width = if cell.flags.contains(Flags::WIDE_CHAR) {
                    2
                } else {
                    1
                };
                let text = if cell.flags.contains(Flags::HIDDEN) {
                    " ".to_owned()
                } else {
                    let mut value = cell.c.to_string();
                    if let Some(zerowidth) = cell.zerowidth() {
                        value.extend(zerowidth);
                    }
                    value
                };
                cells.push((col, width, text, style));
            }

            while cells
                .last()
                .is_some_and(|(_, _, text, style)| text == " " && style.is_default())
            {
                cells.pop();
            }

            let mut runs: Vec<Run> = Vec::new();
            for (col, width, text, style) in cells {
                if let Some(run) = runs.last_mut()
                    && run.col + run.cells == col
                    && same_style(run, style)
                {
                    run.cells += width;
                    run.text.push_str(&text);
                    continue;
                }
                runs.push(Run {
                    col,
                    cells: width,
                    text,
                    fg: style.fg,
                    bg: style.bg,
                    bold: style.bold,
                    italic: style.italic,
                    underline: style.underline,
                    strikeout: style.strikeout,
                });
            }
            lines.push(Line { runs });
        }

        let cursor = if self.term.mode().contains(TermMode::SHOW_CURSOR) {
            let point = self.term.renderable_content().cursor.point;
            let row = point.line.0 + offset as i32;
            (row >= 0 && row < i32::from(self.rows) && point.column.0 < usize::from(self.cols))
                .then_some(Cursor {
                    line: row as u16,
                    col: point.column.0 as u16,
                })
        } else {
            None
        };

        Frame {
            cols: self.cols,
            rows: self.rows,
            lines,
            cursor,
        }
    }
}

#[derive(Clone, Copy)]
struct Style {
    fg: u32,
    bg: Option<u32>,
    bold: bool,
    italic: bool,
    underline: bool,
    strikeout: bool,
}

impl Style {
    fn is_default(self) -> bool {
        self.fg == palette::DEFAULT_FG
            && self.bg.is_none()
            && !self.bold
            && !self.italic
            && !self.underline
            && !self.strikeout
    }
}

fn same_style(run: &Run, style: Style) -> bool {
    run.fg == style.fg
        && run.bg == style.bg
        && run.bold == style.bold
        && run.italic == style.italic
        && run.underline == style.underline
        && run.strikeout == style.strikeout
}

fn style(cell: &Cell) -> Style {
    let mut fg = resolve_color(cell.fg, true).unwrap_or(palette::DEFAULT_FG);
    let mut bg = resolve_color(cell.bg, false);
    if cell.flags.contains(Flags::INVERSE) {
        let old_fg = fg;
        fg = bg.unwrap_or(palette::DEFAULT_BG);
        bg = if old_fg == palette::DEFAULT_BG {
            None
        } else {
            Some(old_fg)
        };
    }
    if cell.flags.contains(Flags::DIM) {
        fg = blend(fg, bg.unwrap_or(palette::DEFAULT_BG));
    }
    Style {
        fg,
        bg,
        bold: cell.flags.contains(Flags::BOLD),
        italic: cell.flags.contains(Flags::ITALIC),
        underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
        strikeout: cell.flags.contains(Flags::STRIKEOUT),
    }
}

fn resolve_color(color: Color, foreground: bool) -> Option<u32> {
    match color {
        Color::Spec(rgb) => {
            Some((u32::from(rgb.r) << 16) | (u32::from(rgb.g) << 8) | u32::from(rgb.b))
        }
        Color::Indexed(index) => Some(palette::indexed(index)),
        Color::Named(named) => match named {
            NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::DimForeground => {
                foreground.then_some(palette::DEFAULT_FG)
            }
            NamedColor::Background => None,
            NamedColor::Cursor => Some(palette::CURSOR),
            NamedColor::DimBlack => Some(palette::ANSI[0]),
            NamedColor::DimRed => Some(palette::ANSI[1]),
            NamedColor::DimGreen => Some(palette::ANSI[2]),
            NamedColor::DimYellow => Some(palette::ANSI[3]),
            NamedColor::DimBlue => Some(palette::ANSI[4]),
            NamedColor::DimMagenta => Some(palette::ANSI[5]),
            NamedColor::DimCyan => Some(palette::ANSI[6]),
            NamedColor::DimWhite => Some(palette::ANSI[7]),
            other => {
                let index = other as usize;
                (index < 16).then_some(palette::ANSI[index])
            }
        },
    }
}

fn blend(fg: u32, bg: u32) -> u32 {
    let channel = |shift| (2_u32 * ((fg >> shift) & 0xff_u32) + ((bg >> shift) & 0xff_u32)) / 3_u32;
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}
