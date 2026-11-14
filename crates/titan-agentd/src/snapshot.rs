use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    index::{Column, Line, Point},
    term::{
        Term, TermMode,
        cell::{Cell, Flags},
    },
    vte::ansi::{Color, NamedColor},
};

fn color(out: &mut Vec<u8>, value: Color, foreground: bool) {
    let code = match value {
        Color::Named(named) => {
            let n = named as u16;
            if n <= 15 {
                Some(if n < 8 {
                    if foreground { 30 + n } else { 40 + n }
                } else if foreground {
                    90 + n - 8
                } else {
                    100 + n - 8
                })
            } else if named
                == if foreground {
                    NamedColor::Foreground
                } else {
                    NamedColor::Background
                }
            {
                Some(if foreground { 39 } else { 49 })
            } else {
                None
            }
        }
        Color::Indexed(n) => {
            out.extend_from_slice(
                format!("\x1b[{};5;{}m", if foreground { 38 } else { 48 }, n).as_bytes(),
            );
            return;
        }
        Color::Spec(rgb) => {
            out.extend_from_slice(
                format!(
                    "\x1b[{};2;{};{};{}m",
                    if foreground { 38 } else { 48 },
                    rgb.r,
                    rgb.g,
                    rgb.b
                )
                .as_bytes(),
            );
            return;
        }
    };
    if let Some(code) = code {
        out.extend_from_slice(format!("\x1b[{code}m").as_bytes());
    }
}

fn style(out: &mut Vec<u8>, cell: &Cell) {
    out.extend_from_slice(b"\x1b[0m");
    for (flag, code) in [
        (Flags::BOLD, 1),
        (Flags::DIM, 2),
        (Flags::ITALIC, 3),
        (Flags::UNDERLINE, 4),
        (Flags::INVERSE, 7),
        (Flags::HIDDEN, 8),
        (Flags::STRIKEOUT, 9),
    ] {
        if cell.flags.contains(flag) {
            out.extend_from_slice(format!("\x1b[{code}m").as_bytes());
        }
    }
    color(out, cell.fg, true);
    color(out, cell.bg, false);
}

pub fn snapshot_ansi<T: EventListener>(term: &Term<T>) -> Vec<u8> {
    let grid = term.grid();
    let mut out = b"\x1b[0m\x1b[3J\x1b[2J\x1b[H".to_vec();
    if term.mode().contains(TermMode::ALT_SCREEN) {
        out.extend_from_slice(b"\x1b[?1049h\x1b[2J\x1b[H");
    }
    for row in 0..grid.screen_lines() {
        out.extend_from_slice(format!("\x1b[{};1H", row + 1).as_bytes());
        for col in 0..grid.columns() {
            let cell = &grid[Point::new(Line(row as i32), Column(col))];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            style(&mut out, cell);
            out.extend_from_slice(cell.c.encode_utf8(&mut [0; 4]).as_bytes());
            if let Some(chars) = cell.zerowidth() {
                for c in chars {
                    out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                }
            }
        }
    }
    let cursor = grid.cursor.point;
    out.extend_from_slice(
        format!("\x1b[{};{}H", cursor.line.0 + 1, cursor.column.0 + 1).as_bytes(),
    );
    if term.mode().contains(TermMode::APP_CURSOR) {
        out.extend_from_slice(b"\x1b[?1h");
    }
    if term.mode().contains(TermMode::BRACKETED_PASTE) {
        out.extend_from_slice(b"\x1b[?2004h");
    }
    if term.mode().contains(TermMode::SHOW_CURSOR) {
        out.extend_from_slice(b"\x1b[?25h");
    } else {
        out.extend_from_slice(b"\x1b[?25l");
    }
    style(&mut out, &grid.cursor.template);
    out.extend_from_slice(b"\x1b[3J");
    out
}
