use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    index::{Column, Line, Point},
    term::{Term, TermMode},
    vte::ansi::Color,
};

pub fn snapshot_ansi<T: EventListener>(term: &Term<T>) -> Vec<u8> {
    let grid = term.grid();
    let mut out = b"\x1b[0m\x1b[2J\x1b[H".to_vec();
    if term.mode().contains(TermMode::ALT_SCREEN) {
        out.extend_from_slice(b"\x1b[?1049h");
    }
    if term.mode().contains(TermMode::APP_CURSOR) {
        out.extend_from_slice(b"\x1b[?1h");
    }
    if term.mode().contains(TermMode::BRACKETED_PASTE) {
        out.extend_from_slice(b"\x1b[?2004h");
    }
    if !term.mode().contains(TermMode::SHOW_CURSOR) {
        out.extend_from_slice(b"\x1b[?25l");
    }
    for row in 0..grid.screen_lines() {
        out.extend_from_slice(format!("\x1b[{};1H", row + 1).as_bytes());
        for col in 0..grid.columns() {
            let cell = &grid[Point::new(Line(row as i32), Column(col))];
            out.extend_from_slice(cell.c.to_string().as_bytes());
            if let Some(chars) = cell.zerowidth() {
                out.extend_from_slice(chars.iter().collect::<String>().as_bytes());
            }
            let _ = (
                cell.fg,
                cell.bg,
                Color::Named(alacritty_terminal::vte::ansi::NamedColor::Foreground),
            );
        }
    }
    let cursor = grid.cursor.point;
    out.extend_from_slice(
        format!("\x1b[{};{}H", cursor.line.0 + 1, cursor.column.0 + 1).as_bytes(),
    );
    let cell = &grid.cursor.template;
    out.extend_from_slice(b"\x1b[0m");
    if cell
        .flags
        .contains(alacritty_terminal::term::cell::Flags::BOLD)
    {
        out.extend_from_slice(b"\x1b[1m");
    }
    if cell
        .flags
        .contains(alacritty_terminal::term::cell::Flags::DIM)
    {
        out.extend_from_slice(b"\x1b[2m");
    }
    if cell
        .flags
        .contains(alacritty_terminal::term::cell::Flags::ITALIC)
    {
        out.extend_from_slice(b"\x1b[3m");
    }
    if cell
        .flags
        .contains(alacritty_terminal::term::cell::Flags::UNDERLINE)
    {
        out.extend_from_slice(b"\x1b[4m");
    }
    out
}
