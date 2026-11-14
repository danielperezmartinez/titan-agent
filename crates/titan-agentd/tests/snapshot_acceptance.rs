use alacritty_terminal::{
    event::VoidListener,
    grid::Dimensions,
    index::{Column, Line, Point},
    term::{
        Term, TermMode,
        cell::{Cell, Flags},
        test::TermSize,
    },
    vte::ansi::{Processor, StdSyncHandler},
};
use titan_agentd::snapshot::snapshot_ansi;

fn terminal() -> Term<VoidListener> {
    Term::new(Default::default(), &TermSize::new(24, 5), VoidListener)
}

fn feed(term: &mut Term<VoidListener>, bytes: &[u8]) {
    Processor::<StdSyncHandler>::new().advance(term, bytes);
}

fn assert_cell(a: &Cell, b: &Cell) {
    let flags = Flags::BOLD
        | Flags::DIM
        | Flags::ITALIC
        | Flags::UNDERLINE
        | Flags::INVERSE
        | Flags::HIDDEN
        | Flags::STRIKEOUT
        | Flags::WIDE_CHAR
        | Flags::WIDE_CHAR_SPACER;
    assert_eq!(a.c, b.c);
    assert_eq!(a.zerowidth(), b.zerowidth());
    assert_eq!(a.fg, b.fg);
    assert_eq!(a.bg, b.bg);
    assert_eq!(a.flags & flags, b.flags & flags);
}

fn assert_visible(a: &Term<VoidListener>, b: &Term<VoidListener>) {
    for row in 0..5 {
        for col in 0..24 {
            let point = Point::new(Line(row), Column(col));
            assert_cell(&a.grid()[point], &b.grid()[point]);
        }
    }
    assert_eq!(a.grid().cursor.point, b.grid().cursor.point);
    let modes = TermMode::SHOW_CURSOR
        | TermMode::APP_CURSOR
        | TermMode::BRACKETED_PASTE
        | TermMode::ALT_SCREEN;
    assert_eq!(*a.mode() & modes, *b.mode() & modes);
    assert_cell(&a.grid().cursor.template, &b.grid().cursor.template);
}

#[test]
fn snapshot_round_trips_visible_cells_cursor_modes_and_pending_style() {
    let cases = [
        "plain text\r\nsecond line\x1b[4;8H",
        "\x1b[31;44mNamed\x1b[91;104mBright\x1b[0mDefault",
        "\x1b[38;5;1;48;5;4mLow\x1b[38;5;201;48;5;237mIndexed",
        "\x1b[38;2;12;34;56;48;2;78;90;123mRGB\x1b[39;49mDefault",
        "\x1b[1mB\x1b[2mD\x1b[3mI\x1b[4mU\x1b[7mV\x1b[8mH\x1b[9mS\x1b[0mN",
        "A界e\u{301}中Z\x1b[3;23H界\x1b[4;1H",
        "\x1b[?25lhidden\x1b[?1h\x1b[?2004h\x1b[2;7H",
        "primary\x1b[?1049h\x1b[32mAlternate界\x1b[?25l\x1b[?1h\x1b[?2004h",
        "\x1b[44m\x1b[2J\x1b[2;4Hcolored blanks\x1b[1;3;38;2;2;4;6m",
    ];
    for input in cases {
        let mut a = terminal();
        feed(&mut a, input.as_bytes());
        let mut b = terminal();
        feed(&mut b, &snapshot_ansi(&a));
        assert_visible(&a, &b);
        // Subsequent output must use the original cursor's active style.
        feed(&mut a, b"Z");
        feed(&mut b, b"Z");
        assert_visible(&a, &b);
    }
}

#[test]
fn snapshot_resets_attributes_clears_screen_and_excludes_scrollback() {
    let mut a = terminal();
    feed(&mut a, b"history-only\r\n1\r\n2\r\n3\r\n4\r\n5\r\n6");
    let bytes = snapshot_ansi(&a);
    assert!(!String::from_utf8_lossy(&bytes).contains("history-only"));
    let mut b = terminal();
    feed(&mut b, b"\x1b[1;3;31;44m\x1b[2Jstale text");
    feed(&mut b, &bytes);
    assert_visible(&a, &b);
    assert_eq!(b.grid().history_size(), 0);
}
