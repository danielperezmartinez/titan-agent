//! Terminal of a session: the bytes come from the PTY that titan-agentd keeps
//! alive, `alacritty_terminal` turns them into a grid and GPUI paints it.

use std::cell::RefCell;
use std::rc::Rc;

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor, Rgb};
use gpui::{
    App, Bounds, ClipboardItem, Context, FocusHandle, Focusable, Font, FontStyle, FontWeight,
    KeyDownEvent, MouseButton, MouseDownEvent, Pixels, ScrollEvent, TextAlign, TextRun, Window,
    canvas, div, fill, font, point, prelude::*, px, size,
};
use titan_agent_core::protocol::{Request, encode_bytes};

use crate::client::DaemonClient;
use crate::theme;
use titan_agent::keys::{KeyModes, keystroke_bytes};

/// Terminal text size and line height (design: 13 px, line-height 1.45).
pub const FONT_SIZE: f32 = 13.0;
pub const LINE_HEIGHT: f32 = 19.0;

#[derive(Clone, Copy)]
struct GridSize {
    cols: usize,
    rows: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

#[derive(Clone, Default)]
struct Listener(Rc<RefCell<Vec<TermEvent>>>);

impl EventListener for Listener {
    fn send_event(&self, event: TermEvent) {
        self.0.borrow_mut().push(event);
    }
}

pub struct TerminalView {
    pub session: String,
    pub terminal: Option<String>,
    term: Term<Listener>,
    parser: Processor,
    listener: Listener,
    client: DaemonClient,
    size: GridSize,
    focus: FocusHandle,
    /// Set when the terminal's process ended.
    pub ended: Option<String>,
    scroll_rest: f32,
}

impl TerminalView {
    pub fn new(session: String, client: DaemonClient, cx: &mut Context<Self>) -> TerminalView {
        let size = GridSize {
            cols: 100,
            rows: 30,
        };
        let listener = Listener::default();
        let config = Config {
            scrolling_history: 5000,
            ..Config::default()
        };
        TerminalView {
            session,
            terminal: None,
            term: Term::new(config, &size, listener.clone()),
            parser: Processor::new(),
            listener,
            client,
            size,
            focus: cx.focus_handle(),
            ended: None,
            scroll_rest: 0.0,
        }
    }

    /// Asks the daemon for this session's terminal. Returns the request id.
    pub fn attach(&mut self) -> u64 {
        self.ended = None;
        self.client.request(Request::Attach {
            session: self.session.clone(),
            cols: self.size.cols as u16,
            rows: self.size.rows as u16,
        })
    }

    pub fn attached(&mut self, terminal: String, created: bool, cx: &mut Context<Self>) {
        if !created || self.terminal.as_deref() != Some(&terminal) {
            // Start clean: the daemon replays the terminal's history.
            self.term = Term::new(
                Config {
                    scrolling_history: 5000,
                    ..Config::default()
                },
                &self.size,
                self.listener.clone(),
            );
            self.parser = Processor::new();
        }
        self.terminal = Some(terminal);
        cx.notify();
    }

    /// Takes a terminal the daemon already opened for this view (a launched
    /// TUI) and tells it the view's size.
    pub fn adopt(&mut self, terminal: String, cx: &mut Context<Self>) {
        self.client.notify(Request::Resize {
            terminal: terminal.clone(),
            cols: self.size.cols as u16,
            rows: self.size.rows as u16,
        });
        self.terminal = Some(terminal);
        self.ended = None;
        cx.notify();
    }

    pub fn feed(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.parser.advance(&mut self.term, bytes);
        let events: Vec<TermEvent> = self.listener.0.borrow_mut().drain(..).collect();
        for event in events {
            match event {
                TermEvent::PtyWrite(text) => self.send(text.as_bytes()),
                TermEvent::ClipboardStore(_, text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text))
                }
                TermEvent::TextAreaSizeRequest(reply) => {
                    let size = alacritty_terminal::event::WindowSize {
                        num_lines: self.size.rows as u16,
                        num_cols: self.size.cols as u16,
                        cell_width: 8,
                        cell_height: LINE_HEIGHT as u16,
                    };
                    self.send(reply(size).as_bytes());
                }
                TermEvent::ColorRequest(index, reply) => {
                    let hex = match index {
                        0..=255 => theme::xterm(index as u8),
                        257 => theme::TERMINAL_BG,
                        _ => theme::BODY,
                    };
                    let rgb = Rgb {
                        r: (hex >> 16) as u8,
                        g: (hex >> 8) as u8,
                        b: hex as u8,
                    };
                    self.send(reply(rgb).as_bytes());
                }
                _ => {}
            }
        }
        cx.notify();
    }

    pub fn closed(&mut self, exit_code: Option<u32>, cx: &mut Context<Self>) {
        self.terminal = None;
        self.ended = Some(match exit_code {
            Some(0) | None => "el terminal se cerró".into(),
            Some(code) => format!("el terminal terminó con el código {code}"),
        });
        cx.notify();
    }

    fn send(&self, bytes: &[u8]) {
        if let Some(terminal) = &self.terminal {
            self.client.notify(Request::Input {
                terminal: terminal.clone(),
                data: encode_bytes(bytes),
            });
        }
    }

    fn resize(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        let cols = cols.max(20);
        let rows = rows.max(5);
        if cols == self.size.cols && rows == self.size.rows {
            return;
        }
        self.size = GridSize { cols, rows };
        self.term.resize(self.size);
        if let Some(terminal) = &self.terminal {
            self.client.notify(Request::Resize {
                terminal: terminal.clone(),
                cols: cols as u16,
                rows: rows as u16,
            });
        }
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
        if m.control && m.shift && keystroke.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.paste(&text);
            }
            cx.stop_propagation();
            return;
        }
        let modes = KeyModes {
            app_cursor: self.term.mode().contains(TermMode::APP_CURSOR),
        };
        if let Some(bytes) = keystroke_bytes(keystroke, modes) {
            self.term.scroll_display(Scroll::Bottom);
            self.send(&bytes);
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        if self.term.mode().contains(TermMode::BRACKETED_PASTE) {
            self.send(format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).as_bytes());
        } else {
            self.send(text.as_bytes());
        }
    }

    fn scroll(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(LINE_HEIGHT)).y / px(LINE_HEIGHT);
        self.scroll_rest += delta;
        let lines = self.scroll_rest.trunc() as i32;
        if lines == 0 {
            return;
        }
        self.scroll_rest -= lines as f32;
        let mode = *self.term.mode();
        if mode.intersects(TermMode::MOUSE_MODE) && mode.contains(TermMode::SGR_MOUSE) {
            let button = if lines > 0 { 64 } else { 65 };
            for _ in 0..lines.unsigned_abs().min(10) {
                self.send(format!("\x1b[<{button};1;1M").as_bytes());
            }
        } else if mode.contains(TermMode::ALT_SCREEN) {
            let arrow = if lines > 0 { "A" } else { "B" };
            let prefix = if mode.contains(TermMode::APP_CURSOR) {
                "\x1bO"
            } else {
                "\x1b["
            };
            for _ in 0..lines.unsigned_abs().min(10) {
                self.send(format!("{prefix}{arrow}").as_bytes());
            }
        } else {
            self.term.scroll_display(Scroll::Delta(lines));
        }
        cx.notify();
    }

    /// Copies what the grid shows into paint-ready rows.
    fn snapshot(&self, focused: bool) -> Snapshot {
        let content = self.term.renderable_content();
        let offset = content.display_offset as i32;
        let colors = content.colors;
        let resolve = |color: Color| -> u32 {
            match color {
                Color::Spec(rgb) => pack(rgb),
                Color::Indexed(index) => colors[index as usize]
                    .map(pack)
                    .unwrap_or_else(|| theme::xterm(index)),
                Color::Named(named) => colors[named as usize]
                    .map(pack)
                    .unwrap_or_else(|| named_color(named)),
            }
        };
        let cursor_row = content.cursor.point.line.0 + offset;
        let cursor = (content.cursor.shape != CursorShape::Hidden
            && cursor_row >= 0
            && (cursor_row as usize) < self.size.rows)
            .then_some(Cursor {
                row: cursor_row as usize,
                col: content.cursor.point.column.0,
                focused,
            });
        let mut rows: Vec<Row> = (0..self.size.rows).map(|_| Row::default()).collect();
        for indexed in content.display_iter {
            let row_index = indexed.point.line.0 + offset;
            if row_index < 0 || row_index as usize >= rows.len() {
                continue;
            }
            let cell = indexed.cell;
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER)
                || cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let col = indexed.point.column.0;
            let mut fg = resolve(cell.fg);
            let mut bg = resolve(cell.bg);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.flags.contains(Flags::DIM) {
                fg = mix(fg, theme::TERMINAL_BG, 0.4);
            }
            if cell.flags.contains(Flags::HIDDEN) {
                fg = bg;
            }
            if cursor
                .as_ref()
                .is_some_and(|c| c.focused && c.row == row_index as usize && c.col == col)
            {
                fg = theme::TERMINAL_BG;
            }
            let wide = cell.flags.contains(Flags::WIDE_CHAR);
            let row = &mut rows[row_index as usize];
            if bg != theme::TERMINAL_BG {
                let width = if wide { 2 } else { 1 };
                match row.backgrounds.last_mut() {
                    Some(last) if last.color == bg && last.col + last.len == col => {
                        last.len += width
                    }
                    _ => row.backgrounds.push(Background {
                        col,
                        len: width,
                        color: bg,
                    }),
                }
            }
            let mut c = cell.c;
            if c == '\t' {
                c = ' ';
            }
            let style = CellStyle {
                fg,
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
                underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
                strike: cell.flags.contains(Flags::STRIKEOUT),
            };
            let next_col = row
                .segments
                .last()
                .map(|segment| segment.col + segment.cells)
                .unwrap_or(usize::MAX);
            let continues =
                next_col == col && !row.segments.last().is_some_and(|segment| segment.ends_wide);
            if !continues {
                row.segments.push(Segment {
                    col,
                    cells: 0,
                    text: String::new(),
                    runs: Vec::new(),
                    ends_wide: false,
                });
            }
            let segment = row.segments.last_mut().expect("segment exists");
            let start = segment.text.len();
            segment.text.push(c);
            if let Some(extra) = cell.zerowidth() {
                for z in extra {
                    segment.text.push(*z);
                }
            }
            let len = segment.text.len() - start;
            match segment.runs.last_mut() {
                Some((run_len, run_style)) if *run_style == style => *run_len += len,
                _ => segment.runs.push((len, style)),
            }
            segment.cells += if wide { 2 } else { 1 };
            segment.ends_wide = wide;
        }
        // Drop trailing blanks so less text is shaped.
        for row in &mut rows {
            row.segments.retain(|segment| {
                !segment.text.chars().all(|c| c == ' ')
                    || segment.runs.iter().any(|(_, s)| s.underline || s.strike)
            });
        }
        Snapshot {
            rows,
            cursor,
            size: self.size,
        }
    }
}

fn pack(rgb: Rgb) -> u32 {
    ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | rgb.b as u32
}

fn mix(a: u32, b: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let x = ((a >> shift) & 0xff) as f32;
        let y = ((b >> shift) & 0xff) as f32;
        ((x + (y - x) * amount).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

fn named_color(named: NamedColor) -> u32 {
    use NamedColor::*;
    match named {
        Foreground => theme::BODY,
        Background => theme::TERMINAL_BG,
        Cursor => theme::ACCENT,
        BrightForeground => theme::INK,
        DimForeground => theme::MUTE,
        Black => theme::ANSI[0],
        Red => theme::ANSI[1],
        Green => theme::ANSI[2],
        Yellow => theme::ANSI[3],
        Blue => theme::ANSI[4],
        Magenta => theme::ANSI[5],
        Cyan => theme::ANSI[6],
        White => theme::ANSI[7],
        BrightBlack => theme::ANSI[8],
        BrightRed => theme::ANSI[9],
        BrightGreen => theme::ANSI[10],
        BrightYellow => theme::ANSI[11],
        BrightBlue => theme::ANSI[12],
        BrightMagenta => theme::ANSI[13],
        BrightCyan => theme::ANSI[14],
        BrightWhite => theme::ANSI[15],
        DimBlack => mix(theme::ANSI[0], theme::TERMINAL_BG, 0.4),
        DimRed => mix(theme::ANSI[1], theme::TERMINAL_BG, 0.4),
        DimGreen => mix(theme::ANSI[2], theme::TERMINAL_BG, 0.4),
        DimYellow => mix(theme::ANSI[3], theme::TERMINAL_BG, 0.4),
        DimBlue => mix(theme::ANSI[4], theme::TERMINAL_BG, 0.4),
        DimMagenta => mix(theme::ANSI[5], theme::TERMINAL_BG, 0.4),
        DimCyan => mix(theme::ANSI[6], theme::TERMINAL_BG, 0.4),
        DimWhite => mix(theme::ANSI[7], theme::TERMINAL_BG, 0.4),
    }
}

#[derive(Clone, Copy, PartialEq)]
struct CellStyle {
    fg: u32,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

struct Segment {
    col: usize,
    cells: usize,
    text: String,
    runs: Vec<(usize, CellStyle)>,
    ends_wide: bool,
}

struct Background {
    col: usize,
    len: usize,
    color: u32,
}

#[derive(Default)]
struct Row {
    segments: Vec<Segment>,
    backgrounds: Vec<Background>,
}

struct Cursor {
    row: usize,
    col: usize,
    focused: bool,
}

struct Snapshot {
    rows: Vec<Row>,
    cursor: Option<Cursor>,
    size: GridSize,
}

fn text_font(style: &CellStyle) -> Font {
    let mut f = font(theme::FONT);
    f.features = gpui::FontFeatures::disable_ligatures();
    if style.bold {
        f.weight = FontWeight::BOLD;
    }
    if style.italic {
        f.style = FontStyle::Italic;
    }
    f
}

/// Cell width of the terminal font at [`FONT_SIZE`].
pub fn cell_width(window: &Window) -> Pixels {
    let text = window.text_system();
    let font_id = text.resolve_font(&font(theme::FONT));
    text.advance(font_id, px(FONT_SIZE), 'm')
        .map(|size| size.width)
        .unwrap_or(px(FONT_SIZE * 0.6))
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        let snapshot = self.snapshot(focused);
        let view = cx.entity();
        div()
            .id("terminal")
            .key_context("Terminal")
            .track_focus(&self.focus)
            .size_full()
            .overflow_hidden()
            .bg(theme::color(theme::TERMINAL_BG))
            .on_key_down(cx.listener(Self::key_down))
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| this.scroll(event, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    cx.notify();
                }),
            )
            .child(
                canvas(
                    move |bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
                        let cw = cell_width(window);
                        let cols = (bounds.size.width / cw).floor().max(1.0) as usize;
                        let rows = (bounds.size.height / px(LINE_HEIGHT)).floor().max(1.0) as usize;
                        if cols != snapshot.size.cols || rows != snapshot.size.rows {
                            view.update(cx, |this, cx| this.resize(cols, rows, cx));
                        }
                        (cw, snapshot)
                    },
                    move |bounds, (cw, snapshot), window, cx| {
                        paint_grid(bounds, cw, &snapshot, window, cx);
                    },
                )
                .size_full(),
            )
    }
}

fn paint_grid(
    bounds: Bounds<Pixels>,
    cw: Pixels,
    snapshot: &Snapshot,
    window: &mut Window,
    cx: &mut App,
) {
    let lh = px(LINE_HEIGHT);
    let origin = bounds.origin;
    for (index, row) in snapshot.rows.iter().enumerate() {
        let y = origin.y + lh * index as f32;
        for bg in &row.backgrounds {
            window.paint_quad(fill(
                Bounds::new(
                    point(origin.x + cw * bg.col as f32, y),
                    size(cw * bg.len as f32, lh),
                ),
                theme::hsla(bg.color),
            ));
        }
    }
    if let Some(cursor) = &snapshot.cursor {
        let bounds = Bounds::new(
            point(
                origin.x + cw * cursor.col as f32,
                origin.y + lh * cursor.row as f32,
            ),
            size(cw, lh),
        );
        if cursor.focused {
            window.paint_quad(fill(bounds, theme::hsla(theme::ACCENT)));
        } else {
            window.paint_quad(gpui::outline(
                bounds,
                theme::hsla(theme::ACCENT),
                gpui::BorderStyle::Solid,
            ));
        }
    }
    for (index, row) in snapshot.rows.iter().enumerate() {
        let y = origin.y + lh * index as f32;
        for segment in &row.segments {
            let runs: Vec<TextRun> = segment
                .runs
                .iter()
                .map(|(len, style)| TextRun {
                    len: *len,
                    font: text_font(style),
                    color: theme::hsla(style.fg),
                    background_color: None,
                    underline: style.underline.then(|| gpui::UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(theme::hsla(style.fg)),
                        wavy: false,
                    }),
                    strikethrough: style.strike.then(|| gpui::StrikethroughStyle {
                        thickness: px(1.0),
                        color: Some(theme::hsla(style.fg)),
                    }),
                })
                .collect();
            let line = window.text_system().shape_line(
                segment.text.clone().into(),
                px(FONT_SIZE),
                &runs,
                Some(cw),
            );
            let _ = line.paint(
                point(origin.x + cw * segment.col as f32, y),
                lh,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
    }
}
