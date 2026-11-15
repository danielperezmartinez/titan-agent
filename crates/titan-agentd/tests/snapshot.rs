use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use titan_agentd::snapshot::{Emulator, ReplyListener, replay};

fn visible_flags() -> Flags {
    Flags::BOLD
        | Flags::DIM
        | Flags::ITALIC
        | Flags::UNDERLINE
        | Flags::DOUBLE_UNDERLINE
        | Flags::UNDERCURL
        | Flags::INVERSE
        | Flags::HIDDEN
        | Flags::STRIKEOUT
        | Flags::WIDE_CHAR
        | Flags::WIDE_CHAR_SPACER
}

fn required_modes() -> TermMode {
    TermMode::ALT_SCREEN
        | TermMode::SHOW_CURSOR
        | TermMode::APP_CURSOR
        | TermMode::APP_KEYPAD
        | TermMode::BRACKETED_PASTE
        | TermMode::FOCUS_IN_OUT
        | TermMode::MOUSE_REPORT_CLICK
        | TermMode::MOUSE_DRAG
        | TermMode::MOUSE_MOTION
        | TermMode::SGR_MOUSE
        | TermMode::UTF8_MOUSE
        | TermMode::LINE_WRAP
}

fn cell(emulator: &Emulator, row: usize, col: usize) -> &Cell {
    &emulator.term().grid()[Line(row as i32)][Column(col)]
}

fn assert_same_screen(original: &Emulator, restored: &Emulator) {
    assert_eq!(original.size(), restored.size());
    let (cols, rows) = original.size();
    for row in 0..usize::from(rows) {
        for col in 0..usize::from(cols) {
            let expected = cell(original, row, col);
            let actual = cell(restored, row, col);
            assert_eq!(actual.c, expected.c, "character at ({row}, {col})");
            assert_eq!(actual.fg, expected.fg, "foreground at ({row}, {col})");
            assert_eq!(actual.bg, expected.bg, "background at ({row}, {col})");
            // Wrapping metadata and attributes outside the contract are excluded.
            assert_eq!(
                actual.flags & visible_flags(),
                expected.flags & visible_flags(),
                "flags at ({row}, {col})"
            );
        }
    }
    assert_eq!(
        restored.term().grid().cursor.point,
        original.term().grid().cursor.point
    );
    assert_eq!(
        *restored.term().mode() & required_modes(),
        *original.term().mode() & required_modes()
    );
}

fn assert_round_trip(original: &Emulator) {
    let bytes = original.snapshot();
    assert!(
        bytes.starts_with(b"\x1bc"),
        "snapshot must begin with a full reset"
    );
    let (cols, rows) = original.size();
    let restored = replay(cols, rows, &bytes);
    assert_same_screen(original, &restored);
}

#[test]
fn public_types_satisfy_thread_and_listener_contracts() {
    fn assert_send<T: Send>() {}
    fn assert_listener<T: alacritty_terminal::event::EventListener + Clone + Default>() {}
    assert_send::<Emulator>();
    assert_listener::<ReplyListener>();
}

#[test]
fn blank_and_plain_text_screens_preserve_cursor_and_spaces() {
    let mut emulator = Emulator::new(14, 4, 0);
    assert_eq!(emulator.size(), (14, 4));
    assert_eq!(cell(&emulator, 0, 0).c, ' ');
    assert_round_trip(&emulator);

    assert!(
        emulator
            .feed(b"alpha  beta\r\nsecond\x1b[4;3Hend\x1b[2;4H")
            .is_empty()
    );
    assert_eq!(cell(&emulator, 0, 0).c, 'a');
    assert_eq!(cell(&emulator, 0, 5).c, ' ');
    assert_eq!(cell(&emulator, 3, 2).c, 'e');
    assert_eq!(emulator.term().grid().cursor.point.line, Line(1));
    assert_eq!(emulator.term().grid().cursor.point.column, Column(3));
    assert_round_trip(&emulator);
}

#[test]
fn all_basic_named_foregrounds_and_backgrounds_round_trip() {
    let colors = [
        NamedColor::Black,
        NamedColor::Red,
        NamedColor::Green,
        NamedColor::Yellow,
        NamedColor::Blue,
        NamedColor::Magenta,
        NamedColor::Cyan,
        NamedColor::White,
        NamedColor::BrightBlack,
        NamedColor::BrightRed,
        NamedColor::BrightGreen,
        NamedColor::BrightYellow,
        NamedColor::BrightBlue,
        NamedColor::BrightMagenta,
        NamedColor::BrightCyan,
        NamedColor::BrightWhite,
    ];
    let mut emulator = Emulator::new(20, 3, 0);
    for (index, color) in colors.into_iter().enumerate() {
        let fg = if index < 8 {
            30 + index
        } else {
            90 + index - 8
        };
        let bg = if index < 8 {
            40 + index
        } else {
            100 + index - 8
        };
        emulator.feed(
            format!(
                "\x1b[1;{}H\x1b[{fg}mF\x1b[2;{}H\x1b[{bg}mB",
                index + 1,
                index + 1
            )
            .as_bytes(),
        );
        assert_eq!(cell(&emulator, 0, index).fg, Color::Named(color));
        assert_eq!(cell(&emulator, 1, index).bg, Color::Named(color));
    }
    emulator.feed(b"\x1b[3;1H\x1b[39;49mD");
    assert_eq!(
        cell(&emulator, 2, 0).fg,
        Color::Named(NamedColor::Foreground)
    );
    assert_eq!(
        cell(&emulator, 2, 0).bg,
        Color::Named(NamedColor::Background)
    );
    assert_round_trip(&emulator);
}

#[test]
fn indexed_and_true_colors_including_boundary_values_round_trip() {
    let mut emulator = Emulator::new(12, 3, 0);
    for (col, index) in [0u8, 15, 16, 127, 231, 255].into_iter().enumerate() {
        emulator.feed(format!("\x1b[38;5;{index};48;5;{index}mX").as_bytes());
        assert_eq!(cell(&emulator, 0, col).fg, Color::Indexed(index));
        assert_eq!(cell(&emulator, 0, col).bg, Color::Indexed(index));
    }
    emulator.feed(b"\x1b[2;1H\x1b[38;2;0;127;255;48;2;255;0;19mR ");
    for col in 0..2 {
        assert_eq!(
            cell(&emulator, 1, col).fg,
            Color::Spec(Rgb {
                r: 0,
                g: 127,
                b: 255
            })
        );
        assert_eq!(
            cell(&emulator, 1, col).bg,
            Color::Spec(Rgb {
                r: 255,
                g: 0,
                b: 19
            })
        );
    }
    emulator.feed(b"\x1b[0mD");
    assert_round_trip(&emulator);
}

#[test]
fn required_attributes_and_style_transitions_round_trip() {
    let attributes = [
        ("1", Flags::BOLD),
        ("2", Flags::DIM),
        ("3", Flags::ITALIC),
        ("4", Flags::UNDERLINE),
        ("4:2", Flags::DOUBLE_UNDERLINE),
        ("4:3", Flags::UNDERCURL),
        ("7", Flags::INVERSE),
        ("8", Flags::HIDDEN),
        ("9", Flags::STRIKEOUT),
    ];
    let mut emulator = Emulator::new(24, 3, 0);
    for (col, (sgr, flag)) in attributes.into_iter().enumerate() {
        emulator.feed(format!("\x1b[0;{sgr}mX").as_bytes());
        assert!(cell(&emulator, 0, col).flags.contains(flag), "SGR {sgr}");
    }
    emulator.feed(b"\x1b[2;1H\x1b[0;1;2;3;4;7;8;9;38;5;203;48;2;8;16;32mAB  \x1b[0mplain");
    assert_round_trip(&emulator);
}

#[test]
fn wide_cjk_and_emoji_preserve_both_cells_and_following_text() {
    let mut emulator = Emulator::new(10, 3, 0);
    emulator.feed("\x1b[1;32;44m界🙂Z\x1b[0m\x1b[2;9H界".as_bytes());
    for (col, character) in [(0, '界'), (2, '🙂')] {
        assert_eq!(cell(&emulator, 0, col).c, character);
        assert!(cell(&emulator, 0, col).flags.contains(Flags::WIDE_CHAR));
        assert!(
            cell(&emulator, 0, col + 1)
                .flags
                .contains(Flags::WIDE_CHAR_SPACER)
        );
    }
    assert_eq!(cell(&emulator, 0, 4).c, 'Z');
    assert!(
        cell(&emulator, 1, 9)
            .flags
            .contains(Flags::WIDE_CHAR_SPACER)
    );
    assert_round_trip(&emulator);
}

#[test]
fn occupied_bottom_right_cell_does_not_scroll_during_snapshot_replay() {
    let mut emulator = Emulator::new(4, 2, 0);
    emulator.feed(b"ABCD\x1b[2;1HEFGH");
    assert_eq!(cell(&emulator, 0, 0).c, 'A');
    assert_eq!(cell(&emulator, 1, 3).c, 'H');
    assert_round_trip(&emulator);
}

#[test]
fn alternate_screen_is_recreated_with_its_own_contents_and_cursor() {
    let mut emulator = Emulator::new(18, 4, 8);
    emulator.feed(b"primary\x1b[?1049h\x1b[2;3H\x1b[31malternate\x1b[3;5H");
    assert!(emulator.term().mode().contains(TermMode::ALT_SCREEN));
    assert_eq!(cell(&emulator, 0, 0).c, ' ');
    assert_eq!(cell(&emulator, 1, 2).c, 'a');
    assert_round_trip(&emulator);
    emulator.feed(b"\x1b[?1049l");
    assert!(!emulator.term().mode().contains(TermMode::ALT_SCREEN));
    assert_eq!(cell(&emulator, 0, 0).c, 'p');
    assert_round_trip(&emulator);
}

#[test]
fn each_required_mode_round_trips_when_enabled_and_disabled() {
    let modes: &[(&[u8], &[u8], TermMode)] = &[
        (b"\x1b[?25h", b"\x1b[?25l", TermMode::SHOW_CURSOR),
        (b"\x1b[?1h", b"\x1b[?1l", TermMode::APP_CURSOR),
        (b"\x1b=", b"\x1b>", TermMode::APP_KEYPAD),
        (b"\x1b[?2004h", b"\x1b[?2004l", TermMode::BRACKETED_PASTE),
        (b"\x1b[?1004h", b"\x1b[?1004l", TermMode::FOCUS_IN_OUT),
        (b"\x1b[?1000h", b"\x1b[?1000l", TermMode::MOUSE_REPORT_CLICK),
        (b"\x1b[?1002h", b"\x1b[?1002l", TermMode::MOUSE_DRAG),
        (b"\x1b[?1003h", b"\x1b[?1003l", TermMode::MOUSE_MOTION),
        (b"\x1b[?1006h", b"\x1b[?1006l", TermMode::SGR_MOUSE),
        (b"\x1b[?1005h", b"\x1b[?1005l", TermMode::UTF8_MOUSE),
        (b"\x1b[?7h", b"\x1b[?7l", TermMode::LINE_WRAP),
    ];
    for &(enable, disable, mode) in modes {
        let mut emulator = replay(12, 3, b"sample\x1b[2;3H");
        emulator.feed(enable);
        assert!(emulator.term().mode().contains(mode), "enable {mode:?}");
        assert_round_trip(&emulator);
        emulator.feed(disable);
        assert!(!emulator.term().mode().contains(mode), "disable {mode:?}");
        assert_round_trip(&emulator);
    }
}

#[test]
fn combined_modes_and_hidden_cursor_round_trip_on_alternate_screen() {
    let mut emulator = Emulator::new(10, 3, 0);
    emulator.feed(b"\x1b[?1049h\x1b[?1;2004;1004;1003;1006;1005h\x1b=\x1b[?25;7l\x1b[3;10HX");
    assert!(!emulator.term().mode().contains(TermMode::SHOW_CURSOR));
    assert!(!emulator.term().mode().contains(TermMode::LINE_WRAP));
    assert_round_trip(&emulator);
}

#[test]
fn feed_returns_ordered_replies_without_repeating_previous_replies() {
    let mut emulator = Emulator::new(16, 4, 0);
    assert_eq!(emulator.feed(b"\x1b[6n"), b"\x1b[1;1R");
    assert!(emulator.feed(b"plain").is_empty());
    assert_eq!(emulator.feed(b"\x1b[c"), b"\x1b[?6c");
    assert_eq!(
        emulator.feed(b"\x1b[3;7H\x1b[6n\x1b[c\x1b[1;2H\x1b[6n"),
        b"\x1b[3;7R\x1b[?6c\x1b[1;2R"
    );
    assert!(emulator.feed(b"").is_empty());
    assert!(emulator.feed(b"more").is_empty());
}

#[test]
fn unrelated_events_are_ignored_and_do_not_become_pty_replies() {
    let mut emulator = Emulator::new(12, 3, 0);
    assert!(
        emulator
            .feed(b"\x1b]2;Example title\x07\x07\x1b[18t")
            .is_empty()
    );
    assert_eq!(cell(&emulator, 0, 0).c, ' ');
    assert_eq!(emulator.feed(b"\x1b[6n"), b"\x1b[1;1R");
    assert_round_trip(&emulator);
}

#[test]
fn bytewise_input_matches_whole_input_for_utf8_escapes_and_replies() {
    let input = "start\r\n\x1b[38;2;12;34;56m界🙂\x1b[0m\x1b[6n\x1b[c\x1b[?1049h\x1b[2;2H\x1b[4:3m界🙂\x1b[?25l\x1b[6n";
    let mut whole = Emulator::new(20, 4, 6);
    let expected_replies = whole.feed(input.as_bytes());
    assert!(!expected_replies.is_empty());
    let mut chunked = Emulator::new(20, 4, 6);
    let mut replies = Vec::new();
    for byte in input.as_bytes() {
        replies.extend(chunked.feed(std::slice::from_ref(byte)));
    }
    assert_eq!(replies, expected_replies);
    assert_same_screen(&whole, &chunked);
    assert_round_trip(&chunked);
}

#[test]
fn zero_dimensions_are_clamped_independently_and_one_cell_round_trips() {
    for (cols, rows, expected) in [(0, 0, (1, 1)), (0, 3, (1, 3)), (4, 0, (4, 1))] {
        let mut emulator = Emulator::new(cols, rows, 0);
        assert_eq!(emulator.size(), expected);
        assert_round_trip(&emulator);
        emulator.feed(b"X");
        assert_eq!(cell(&emulator, 0, 0).c, 'X');
        assert_round_trip(&emulator);
    }
    let restored = replay(0, 0, b"Q");
    assert_eq!(restored.size(), (1, 1));
    assert_eq!(cell(&restored, 0, 0).c, 'Q');
}

#[test]
fn resize_updates_dimensions_preserves_text_and_clamps_zero_dimensions() {
    let mut emulator = replay(8, 3, b"abc\x1b[2;2Hz");
    emulator.resize(12, 5);
    assert_eq!(emulator.size(), (12, 5));
    assert_eq!(cell(&emulator, 0, 0).c, 'a');
    assert_eq!(cell(&emulator, 1, 1).c, 'z');
    assert_round_trip(&emulator);
    emulator.resize(5, 3);
    assert_eq!(emulator.size(), (5, 3));
    assert_eq!(cell(&emulator, 0, 2).c, 'c');
    assert_round_trip(&emulator);
    for (cols, rows, expected) in [(0, 2, (1, 2)), (3, 0, (3, 1)), (0, 0, (1, 1))] {
        emulator.resize(cols, rows);
        assert_eq!(emulator.size(), expected);
        emulator.feed(b"\x1b[HX");
        assert_eq!(cell(&emulator, 0, 0).c, 'X');
        assert_round_trip(&emulator);
    }
}

#[test]
fn snapshot_reproduces_visible_rows_after_scrollback_and_leaves_source_unchanged() {
    let input = b"old-a\r\nold-b\r\nold-c\r\nvisible-a\r\nvisible-b\r\nvisible-c\x1b[2;4H";
    let mut original = Emulator::new(14, 3, 20);
    original.feed(input);
    let mut reference = Emulator::new(14, 3, 20);
    reference.feed(input);
    assert_eq!(cell(&original, 0, 0).c, 'v');
    assert_eq!(cell(&original, 2, 8).c, 'c');
    assert_round_trip(&original);
    assert_round_trip(&original);
    assert_same_screen(&reference, &original);
    assert_eq!(original.feed(b"\x1b[6n"), b"\x1b[2;4R");
}
