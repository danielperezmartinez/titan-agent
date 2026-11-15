//! Headless terminal emulator and its ANSI snapshot (encargo v2-instantanea).

use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// Event listener of the emulator: collects `Event::PtyWrite` replies.
#[derive(Clone, Default)]
pub struct ReplyListener {
    replies: Arc<Mutex<Vec<u8>>>,
}

impl EventListener for ReplyListener {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(reply) = event {
            self.replies
                .lock()
                .unwrap()
                .extend_from_slice(reply.as_bytes());
        }
    }
}

/// Headless terminal emulator fed with the raw output of a PTY.
pub struct Emulator {
    term: Term<ReplyListener>,
    processor: Processor,
    listener: ReplyListener,
}

struct Size(usize, usize);

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.1
    }
    fn screen_lines(&self) -> usize {
        self.1
    }
    fn columns(&self) -> usize {
        self.0
    }
}

impl Emulator {
    /// New blank emulator. `cols`/`rows` are clamped to at least 1.
    pub fn new(cols: u16, rows: u16, scrollback: usize) -> Emulator {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let listener = ReplyListener::default();
        let config = Config {
            scrolling_history: scrollback,
            ..Config::default()
        };
        let term = Term::new(config, &Size(cols.into(), rows.into()), listener.clone());
        Emulator {
            term,
            processor: Processor::new(),
            listener,
        }
    }

    /// Feeds raw program output and returns terminal replies.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.listener.replies.lock().unwrap().clear();
        self.processor.advance(&mut self.term, bytes);
        std::mem::take(&mut *self.listener.replies.lock().unwrap())
    }

    /// Resizes the grid (clamped to at least 1x1).
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term
            .resize(Size(cols.max(1).into(), rows.max(1).into()));
    }

    /// Returns the current grid size.
    pub fn size(&self) -> (u16, u16) {
        (
            self.term.columns().try_into().unwrap(),
            self.term.screen_lines().try_into().unwrap(),
        )
    }

    /// Read access to the terminal state.
    pub fn term(&self) -> &Term<ReplyListener> {
        &self.term
    }

    /// ANSI bytes that reproduce the visible screen in a fresh terminal.
    pub fn snapshot(&self) -> Vec<u8> {
        let (cols, rows) = self.size();
        let mode = *self.term.mode();
        let mut out = b"\x1bc".to_vec();
        if mode.contains(TermMode::ALT_SCREEN) {
            out.extend_from_slice(b"\x1b[?1049h");
        }
        out.extend_from_slice(b"\x1b[?7l");
        let mut previous: Option<Cell> = None;
        for row in 0..rows {
            push_fmt(&mut out, format_args!("\x1b[{};1H", row + 1));
            for col in 0..cols {
                let cell = &self.term.grid()[Line(row.into())][Column(col.into())];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let style_changed = previous.as_ref().is_none_or(|old| {
                    old.fg != cell.fg
                        || old.bg != cell.bg
                        || (old.flags & style_flags()) != (cell.flags & style_flags())
                });
                if style_changed {
                    emit_style(&mut out, cell);
                }
                let mut buf = [0; 4];
                out.extend_from_slice(cell.c.encode_utf8(&mut buf).as_bytes());
                previous = Some(cell.clone());
            }
        }
        out.extend_from_slice(b"\x1b[0m");
        let cursor = self.term.grid().cursor.point;
        push_fmt(
            &mut out,
            format_args!("\x1b[{};{}H", cursor.line + 1, cursor.column + 1),
        );
        emit_mode(&mut out, mode, TermMode::SHOW_CURSOR, 25);
        emit_mode(&mut out, mode, TermMode::APP_CURSOR, 1);
        emit_mode(&mut out, mode, TermMode::BRACKETED_PASTE, 2004);
        emit_mode(&mut out, mode, TermMode::FOCUS_IN_OUT, 1004);
        emit_mode(&mut out, mode, TermMode::MOUSE_REPORT_CLICK, 1000);
        emit_mode(&mut out, mode, TermMode::MOUSE_DRAG, 1002);
        emit_mode(&mut out, mode, TermMode::MOUSE_MOTION, 1003);
        emit_mode(&mut out, mode, TermMode::UTF8_MOUSE, 1005);
        emit_mode(&mut out, mode, TermMode::SGR_MOUSE, 1006);
        if mode.contains(TermMode::APP_KEYPAD) {
            out.extend_from_slice(b"\x1b=");
        }
        if mode.contains(TermMode::LINE_WRAP) {
            out.extend_from_slice(b"\x1b[?7h");
        }
        out
    }
}

/// Creates an emulator and feeds it bytes, discarding replies.
pub fn replay(cols: u16, rows: u16, bytes: &[u8]) -> Emulator {
    let mut emulator = Emulator::new(cols, rows, 0);
    emulator.feed(bytes);
    emulator
}

fn style_flags() -> Flags {
    Flags::BOLD
        | Flags::DIM
        | Flags::ITALIC
        | Flags::UNDERLINE
        | Flags::DOUBLE_UNDERLINE
        | Flags::UNDERCURL
        | Flags::INVERSE
        | Flags::HIDDEN
        | Flags::STRIKEOUT
}

fn push_fmt(out: &mut Vec<u8>, args: std::fmt::Arguments<'_>) {
    use std::fmt::Write;
    let mut text = String::new();
    text.write_fmt(args).unwrap();
    out.extend_from_slice(text.as_bytes());
}

fn emit_style(out: &mut Vec<u8>, cell: &Cell) {
    let mut codes = vec!["0".to_owned()];
    for (flag, code) in [
        (Flags::BOLD, 1),
        (Flags::DIM, 2),
        (Flags::ITALIC, 3),
        (Flags::UNDERLINE, 4),
        (Flags::DOUBLE_UNDERLINE, 4),
        (Flags::UNDERCURL, 4),
        (Flags::INVERSE, 7),
        (Flags::HIDDEN, 8),
        (Flags::STRIKEOUT, 9),
    ] {
        if cell.flags.contains(flag) {
            codes.push(if flag == Flags::DOUBLE_UNDERLINE {
                "4:2".into()
            } else if flag == Flags::UNDERCURL {
                "4:3".into()
            } else {
                code.to_string()
            });
        }
    }
    color_codes(&mut codes, cell.fg, true);
    color_codes(&mut codes, cell.bg, false);
    push_fmt(out, format_args!("\x1b[{}m", codes.join(";")));
}

fn color_codes(codes: &mut Vec<String>, color: Color, foreground: bool) {
    let base = if foreground { 30 } else { 40 };
    match color {
        Color::Named(named) => {
            let code = match named {
                NamedColor::Black => Some(base),
                NamedColor::Red => Some(base + 1),
                NamedColor::Green => Some(base + 2),
                NamedColor::Yellow => Some(base + 3),
                NamedColor::Blue => Some(base + 4),
                NamedColor::Magenta => Some(base + 5),
                NamedColor::Cyan => Some(base + 6),
                NamedColor::White => Some(base + 7),
                NamedColor::BrightBlack => Some(base + 60),
                NamedColor::BrightRed => Some(base + 61),
                NamedColor::BrightGreen => Some(base + 62),
                NamedColor::BrightYellow => Some(base + 63),
                NamedColor::BrightBlue => Some(base + 64),
                NamedColor::BrightMagenta => Some(base + 65),
                NamedColor::BrightCyan => Some(base + 66),
                NamedColor::BrightWhite => Some(base + 67),
                NamedColor::Foreground if foreground => Some(39),
                NamedColor::Background if !foreground => Some(49),
                _ => Some(if foreground { 39 } else { 49 }),
            };
            codes.push(code.unwrap().to_string());
        }
        Color::Indexed(index) => {
            codes.push(format!("{};5;{index}", if foreground { 38 } else { 48 }))
        }
        Color::Spec(rgb) => codes.push(format!(
            "{};2;{};{};{}",
            if foreground { 38 } else { 48 },
            rgb.r,
            rgb.g,
            rgb.b
        )),
    }
}

fn emit_mode(out: &mut Vec<u8>, mode: TermMode, flag: TermMode, number: u16) {
    push_fmt(
        out,
        format_args!(
            "\x1b[?{}{}",
            number,
            if mode.contains(flag) { 'h' } else { 'l' }
        ),
    );
}
