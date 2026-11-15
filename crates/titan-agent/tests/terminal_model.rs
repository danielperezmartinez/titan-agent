use titan_agent::terminal_model::{palette, Cursor, Frame, Modes, Run, TerminalModel};

const ANSI: [u32; 16] = [
    0x201D1D, 0xFF453A, 0x30D158, 0xFF9F0A, 0x0A84FF, 0xBF5AF2, 0x5AC8FA, 0xD8D5D5, 0x6E6E73,
    0xFF6961, 0x66D97E, 0xFFB340, 0x409CFF, 0xDA8FFF, 0x8FE0FF, 0xFDFCFC,
];

fn plain(col: u16, cells: u16, text: &str) -> Run {
    Run {
        col,
        cells,
        text: text.to_owned(),
        fg: 0xD8D5D5,
        bg: None,
        bold: false,
        italic: false,
        underline: false,
        strikeout: false,
    }
}

fn rendered(input: &[u8]) -> Frame {
    let mut terminal = TerminalModel::new(20, 2);
    terminal.feed(input);
    terminal.frame()
}

fn dim(fg: u32, bg: u32) -> u32 {
    let channel = |shift| (((2 * ((fg >> shift) & 255)) + ((bg >> shift) & 255)) / 3) << shift;
    channel(16) | channel(8) | channel(0)
}

#[test]
fn palette_tokens_and_all_indexed_entries_are_exact() {
    assert_eq!(palette::DEFAULT_FG, 0xD8D5D5);
    assert_eq!(palette::DEFAULT_BG, 0x161313);
    assert_eq!(palette::CURSOR, 0x0A84FF);
    assert_eq!(palette::ANSI, ANSI);
    for index in 0u8..=255 {
        let expected = match index {
            0..=15 => ANSI[usize::from(index)],
            16..=231 => {
                let levels = [0u32, 95, 135, 175, 215, 255];
                let n = usize::from(index - 16);
                (levels[n / 36] << 16) | (levels[(n / 6) % 6] << 8) | levels[n % 6]
            }
            232..=255 => u32::from(8 + 10 * (index - 232)) * 0x010101,
        };
        assert_eq!(palette::indexed(index), expected, "palette index {index}");
    }
}

#[test]
fn blank_terminal_and_zero_dimensions_have_a_visible_cursor() {
    for (cols, rows) in [(0, 0), (0, 3), (4, 0), (8, 3)] {
        let terminal = TerminalModel::new(cols, rows);
        let expected = (cols.max(1), rows.max(1));
        assert_eq!(terminal.size(), expected);
        assert_eq!(terminal.modes(), Modes::default());
        assert_eq!(terminal.display_offset(), 0);
        let frame = terminal.frame();
        assert_eq!((frame.cols, frame.rows), expected);
        assert_eq!(frame.lines.len(), usize::from(expected.1));
        assert!(frame.lines.iter().all(|line| line.runs.is_empty()));
        assert_eq!(frame.cursor, Some(Cursor { line: 0, col: 0 }));
    }
}

#[test]
fn text_and_carriage_return_line_feed_produce_grid_coordinates() {
    let frame = rendered(b"alpha\r\nbeta");
    assert_eq!(frame.lines[0].runs, vec![plain(0, 5, "alpha")]);
    assert_eq!(frame.lines[1].runs, vec![plain(0, 4, "beta")]);
    assert_eq!(frame.cursor, Some(Cursor { line: 1, col: 4 }));
    let frame = rendered(b"ab\nc");
    assert_eq!(frame.lines[1].runs, vec![plain(0, 3, "  c")]);
    assert_eq!(frame.cursor, Some(Cursor { line: 1, col: 3 }));
}

#[test]
fn normal_and_bright_named_colors_resolve_for_foreground_and_background() {
    for (index, rgb) in ANSI.into_iter().enumerate() {
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
        let frame = rendered(format!("\x1b[{fg};{bg}mX").as_bytes());
        let mut expected = plain(0, 1, "X");
        expected.fg = rgb;
        expected.bg = Some(rgb);
        assert_eq!(frame.lines[0].runs, vec![expected], "named color {index}");
    }
}

#[test]
fn indexed_colors_resolve_in_ansi_cube_and_gray_ranges() {
    for index in [0, 7, 8, 15, 16, 17, 52, 123, 231, 232, 243, 255] {
        let frame = rendered(format!("\x1b[38;5;{index};48;5;{index}mX").as_bytes());
        let mut expected = plain(0, 1, "X");
        expected.fg = palette::indexed(index);
        expected.bg = Some(palette::indexed(index));
        assert_eq!(frame.lines[0].runs, vec![expected], "indexed color {index}");
    }
}

#[test]
fn truecolor_and_default_color_resets_resolve_to_rgb() {
    let frame = rendered(b"\x1b[38;2;17;34;51;48;2;68;85;102mA\x1b[39;49mB");
    let mut colored = plain(0, 1, "A");
    colored.fg = 0x112233;
    colored.bg = Some(0x445566);
    assert_eq!(frame.lines[0].runs, vec![colored, plain(1, 1, "B")]);
}

#[test]
fn bold_preserves_both_default_and_named_foreground_colors() {
    for (sgr, fg) in [("1", palette::DEFAULT_FG), ("31;1", ANSI[1])] {
        let frame = rendered(format!("\x1b[{sgr}mX").as_bytes());
        let mut expected = plain(0, 1, "X");
        expected.bold = true;
        expected.fg = fg;
        assert_eq!(frame.lines[0].runs, vec![expected]);
    }
}

#[test]
fn inverse_swaps_resolved_colors_and_normalizes_default_background() {
    for (input, fg, bg) in [
        ("\x1b[7mX", palette::DEFAULT_BG, Some(palette::DEFAULT_FG)),
        ("\x1b[31;44;7mX", ANSI[4], Some(ANSI[1])),
        ("\x1b[38;2;22;19;19;7mX", palette::DEFAULT_BG, None),
    ] {
        let frame = rendered(input.as_bytes());
        let mut expected = plain(0, 1, "X");
        expected.fg = fg;
        expected.bg = bg;
        assert_eq!(frame.lines[0].runs, vec![expected]);
    }
}

#[test]
fn dim_blends_foreground_with_effective_background_using_integer_division() {
    for (input, fg, bg) in [
        ("\x1b[2mX", palette::DEFAULT_FG, None),
        ("\x1b[31;2mX", ANSI[1], None),
        (
            "\x1b[38;2;17;34;51;48;2;68;85;102;2mX",
            0x112233,
            Some(0x445566),
        ),
    ] {
        let frame = rendered(input.as_bytes());
        let mut expected = plain(0, 1, "X");
        expected.fg = dim(fg, bg.unwrap_or(palette::DEFAULT_BG));
        expected.bg = bg;
        assert_eq!(frame.lines[0].runs, vec![expected]);
    }
}

#[test]
fn hidden_text_becomes_spaces_and_keeps_its_background() {
    let frame = rendered(b"\x1b[41;8msecret\x1b[0m!");
    let mut hidden = plain(0, 6, "      ");
    hidden.bg = Some(ANSI[1]);
    assert_eq!(frame.lines[0].runs, vec![hidden, plain(6, 1, "!")]);
}

#[test]
fn hidden_text_with_default_colors_becomes_internal_spaces() {
    let frame = rendered(b"A\x1b[8msecret\x1b[28mB");
    assert_eq!(frame.lines[0].runs, vec![plain(0, 8, "A      B")]);
}

#[test]
fn runs_merge_equal_styles_and_split_on_each_style_attribute() {
    let frame = rendered(b"A\x1b[39mB\x1b[1mC\x1b[0;3mD\x1b[0;4mE\x1b[0;9mF\x1b[0mG");
    let mut bold = plain(2, 1, "C");
    bold.bold = true;
    let mut italic = plain(3, 1, "D");
    italic.italic = true;
    let mut underline = plain(4, 1, "E");
    underline.underline = true;
    let mut strikeout = plain(5, 1, "F");
    strikeout.strikeout = true;
    assert_eq!(
        frame.lines[0].runs,
        vec![
            plain(0, 2, "AB"),
            bold,
            italic,
            underline,
            strikeout,
            plain(6, 1, "G")
        ]
    );
}

#[test]
fn single_double_and_curly_underlines_set_the_underline_flag() {
    for sgr in ["4", "21", "4:3"] {
        let frame = rendered(format!("\x1b[{sgr}mX").as_bytes());
        let mut expected = plain(0, 1, "X");
        expected.underline = true;
        assert_eq!(frame.lines[0].runs, vec![expected], "underline {sgr}");
    }
}

#[test]
fn default_trailing_spaces_are_trimmed_but_internal_spaces_are_kept() {
    let frame = rendered(b"  a b   \r\n    ");
    assert_eq!(frame.lines[0].runs, vec![plain(0, 5, "  a b")]);
    assert!(frame.lines[1].runs.is_empty());
}

#[test]
fn colored_background_spaces_survive_at_the_end_of_a_line() {
    let mut terminal = TerminalModel::new(5, 2);
    terminal.feed(b"A\x1b[44m    \x1b[0m");
    let mut background = plain(1, 4, "    ");
    background.bg = Some(ANSI[4]);
    assert_eq!(
        terminal.frame().lines[0].runs,
        vec![plain(0, 1, "A"), background]
    );
}

#[test]
fn styled_trailing_spaces_are_not_trimmed_as_default_cells() {
    for sgr in ["31", "1", "3", "4", "9"] {
        let frame = rendered(format!("\x1b[{sgr}mX  \x1b[0m").as_bytes());
        assert_eq!(frame.lines[0].runs.len(), 1);
        assert_eq!(frame.lines[0].runs[0].text, "X  ");
        assert_eq!(frame.lines[0].runs[0].cells, 3);
    }
}

#[test]
fn wide_characters_count_two_cells_without_emitting_spacers() {
    let frame = rendered("A界B\x1b[31m語Z".as_bytes());
    let mut red = plain(4, 3, "語Z");
    red.fg = ANSI[1];
    assert_eq!(frame.lines[0].runs, vec![plain(0, 4, "A界B"), red]);
    assert_eq!(frame.cursor, Some(Cursor { line: 0, col: 7 }));
}

#[test]
fn cursor_position_and_visibility_follow_terminal_sequences() {
    let mut terminal = TerminalModel::new(8, 4);
    terminal.feed(b"\x1b[3;5H");
    assert_eq!(terminal.frame().cursor, Some(Cursor { line: 2, col: 4 }));
    terminal.feed(b"\x1b[?25l");
    assert_eq!(terminal.frame().cursor, None);
    terminal.feed(b"\x1b[?25h");
    assert_eq!(terminal.frame().cursor, Some(Cursor { line: 2, col: 4 }));
}

#[test]
fn keyboard_and_alternate_screen_modes_can_be_enabled_and_disabled() {
    let mut terminal = TerminalModel::new(8, 3);
    terminal.feed(b"\x1b[?1h\x1b[?2004h\x1b[?1049h");
    assert_eq!(
        terminal.modes(),
        Modes {
            app_cursor: true,
            bracketed_paste: true,
            alt_screen: true,
            mouse_reporting: false
        }
    );
    terminal.feed(b"\x1b[?1l\x1b[?2004l\x1b[?1049l");
    assert_eq!(terminal.modes(), Modes::default());
}

#[test]
fn each_mouse_reporting_mode_is_exposed() {
    for mode in [1000, 1002, 1003] {
        let mut terminal = TerminalModel::new(8, 3);
        terminal.feed(format!("\x1b[?{mode}h").as_bytes());
        assert!(terminal.modes().mouse_reporting, "enabled {mode}");
        terminal.feed(format!("\x1b[?{mode}l").as_bytes());
        assert!(!terminal.modes().mouse_reporting, "disabled {mode}");
    }
}

#[test]
fn alternate_screen_restores_the_primary_screen_on_exit() {
    let mut terminal = TerminalModel::new(12, 3);
    terminal.feed(b"primary");
    let primary = terminal.frame();
    terminal.feed(b"\x1b[?1049h\x1b[Hsecondary");
    assert_eq!(
        terminal.frame().lines[0].runs,
        vec![plain(0, 9, "secondary")]
    );
    terminal.feed(b"\x1b[?1049l");
    assert_eq!(terminal.frame(), primary);
}

#[test]
fn snapshot_discards_history_modes_styles_and_partial_parser_state() {
    let mut terminal = TerminalModel::new(10, 2);
    terminal.feed(b"old\r\nold\r\nold\r\nold\x1b[31;1m\x1b[?1h\x1b[?2004h\x1b[?25l");
    terminal.scroll(2);
    assert!(terminal.display_offset() > 0);
    terminal.feed(b"\x1b[38;2;");
    terminal.load_snapshot(12, 3, b"\x1b[32mfresh\x1b[0m\r\nnext");
    let mut fresh = TerminalModel::new(12, 3);
    fresh.feed(b"\x1b[32mfresh\x1b[0m\r\nnext");
    assert_eq!(terminal.size(), (12, 3));
    assert_eq!(terminal.frame(), fresh.frame());
    assert_eq!(terminal.modes(), Modes::default());
    assert_eq!(terminal.display_offset(), 0);
    terminal.scroll(i32::MAX);
    assert_eq!(terminal.display_offset(), 0);
    terminal.load_snapshot(0, 0, b"");
    assert_eq!(terminal.size(), (1, 1));
    assert!(terminal.frame().lines[0].runs.is_empty());
}

#[test]
fn snapshot_is_followed_by_raw_output() {
    let mut terminal = TerminalModel::new(1, 1);
    terminal.load_snapshot(20, 2, b"\x1b[31mseed");
    terminal.feed(b"ling\x1b[0m!");
    let mut red = plain(0, 8, "seedling");
    red.fg = ANSI[1];
    assert_eq!(terminal.frame().lines[0].runs, vec![red, plain(8, 1, "!")]);
}

#[test]
fn snapshot_discards_the_alternate_screen_and_mouse_reporting() {
    let mut terminal = TerminalModel::new(12, 3);
    terminal.feed(b"primary\x1b[?1049h\x1b[?1003halternate");
    assert!(terminal.modes().alt_screen);
    assert!(terminal.modes().mouse_reporting);
    terminal.load_snapshot(12, 3, b"fresh");
    assert_eq!(terminal.modes(), Modes::default());
    assert_eq!(terminal.frame().lines[0].runs, vec![plain(0, 5, "fresh")]);
}

#[test]
fn feeding_can_split_at_every_escape_or_utf8_byte_boundary() {
    let input = "a界é\x1b[38;2;17;34;51m語\x1b[0m\r\nnext\x1b[?2004h".as_bytes();
    let mut whole = TerminalModel::new(20, 4);
    whole.feed(input);
    for split in 0..=input.len() {
        let mut terminal = TerminalModel::new(20, 4);
        terminal.feed(&input[..split]);
        terminal.feed(&[]);
        terminal.feed(&input[split..]);
        assert_eq!(terminal.frame(), whole.frame(), "split at byte {split}");
        assert_eq!(terminal.modes(), whole.modes());
    }
    let mut terminal = TerminalModel::new(20, 4);
    for byte in input {
        terminal.feed(std::slice::from_ref(byte));
    }
    assert_eq!(terminal.frame(), whole.frame());
    assert_eq!(terminal.modes(), whole.modes());
}

#[test]
fn terminal_queries_do_not_appear_as_program_output() {
    let frame = rendered(b"A\x1b[5n\x1b[6n\x1b[c\x1b[>cB");
    assert_eq!(frame.lines[0].runs, vec![plain(0, 2, "AB")]);
    assert_eq!(frame.cursor, Some(Cursor { line: 0, col: 2 }));
}

#[test]
fn resize_changes_frame_dimensions_and_clamps_zero_sizes() {
    let mut terminal = TerminalModel::new(8, 2);
    terminal.feed(b"abc");
    terminal.resize(12, 4);
    let frame = terminal.frame();
    assert_eq!(terminal.size(), (12, 4));
    assert_eq!((frame.cols, frame.rows), (12, 4));
    assert_eq!(frame.lines.len(), 4);
    assert_eq!(frame.lines[0].runs, vec![plain(0, 3, "abc")]);
    for (cols, rows) in [(0, 2), (3, 0), (0, 0)] {
        terminal.resize(cols, rows);
        let frame = terminal.frame();
        assert_eq!(terminal.size(), (cols.max(1), rows.max(1)));
        assert_eq!((frame.cols, frame.rows), terminal.size());
        assert_eq!(frame.lines.len(), usize::from(rows.max(1)));
        if let Some(cursor) = frame.cursor {
            assert!(cursor.col < frame.cols);
            assert!(cursor.line < frame.rows);
        }
    }
}

#[test]
fn history_scrolling_clamps_both_directions_and_restores_the_bottom() {
    let mut terminal = TerminalModel::new(10, 2);
    terminal.feed(b"zero\r\none\r\ntwo\r\nthree\r\nfour");
    let bottom = terminal.frame();
    assert_eq!(bottom.lines[0].runs, vec![plain(0, 5, "three")]);
    assert_eq!(bottom.lines[1].runs, vec![plain(0, 4, "four")]);
    terminal.scroll(0);
    assert_eq!(terminal.frame(), bottom);
    terminal.scroll(1);
    assert_eq!(terminal.display_offset(), 1);
    assert_eq!(terminal.frame().lines[0].runs, vec![plain(0, 3, "two")]);
    assert_eq!(terminal.frame().cursor, None);
    terminal.scroll(i32::MAX);
    assert_eq!(terminal.display_offset(), 3);
    assert_eq!(terminal.frame().lines[0].runs, vec![plain(0, 4, "zero")]);
    assert_eq!(terminal.frame().lines[1].runs, vec![plain(0, 3, "one")]);
    assert_eq!(terminal.frame().cursor, None);
    terminal.scroll(-1);
    assert_eq!(terminal.display_offset(), 2);
    terminal.scroll(i32::MIN);
    assert_eq!(terminal.display_offset(), 0);
    assert_eq!(terminal.frame(), bottom);
    terminal.scroll(2);
    terminal.scroll_to_bottom();
    assert_eq!(terminal.display_offset(), 0);
    assert_eq!(terminal.frame(), bottom);
    terminal.scroll_to_bottom();
    assert_eq!(terminal.frame(), bottom);
}

#[test]
fn scrollback_is_limited_to_ten_thousand_lines() {
    let mut terminal = TerminalModel::new(4, 2);
    for _ in 0..10_020 {
        terminal.feed(b"x\r\n");
    }
    terminal.scroll(i32::MAX);
    assert_eq!(terminal.display_offset(), 10_000);
    assert_eq!(terminal.frame().lines.len(), 2);
    assert_eq!(terminal.frame().cursor, None);
    terminal.scroll_to_bottom();
    assert_eq!(terminal.display_offset(), 0);
    assert!(terminal.frame().cursor.is_some());
}

#[test]
fn cursor_remains_visible_when_its_row_is_still_in_the_scrolled_view() {
    let mut terminal = TerminalModel::new(10, 3);
    terminal.feed(b"zero\r\none\r\ntwo\r\nthree\x1b[1;2H");
    terminal.scroll(1);
    assert_eq!(terminal.display_offset(), 1);
    assert_eq!(terminal.frame().cursor, Some(Cursor { line: 1, col: 1 }));
}
