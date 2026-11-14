use titan_agent::terminal_model::{Span, StyledLine, TerminalModel};
use titan_agent::theme;

fn text(line: &StyledLine) -> String {
    line.spans.iter().map(|span| span.text.as_str()).collect()
}

fn first_span(sequence: &[u8]) -> Span {
    let mut model = TerminalModel::new(12, 2);
    model.feed(sequence);
    model.lines()[0].spans[0].clone()
}

#[test]
fn blank_screen_and_plain_text_have_exact_dimensions_and_default_style() {
    let mut model = TerminalModel::new(8, 3);
    assert_eq!(model.size(), (8, 3));
    assert_eq!(model.cursor(), Some((0, 0)));
    assert_eq!(model.lines().len(), 3);
    for line in model.lines() {
        assert_eq!(
            line.spans,
            vec![Span {
                text: "        ".into(),
                fg: theme::TERMINAL_FG,
                bg: theme::TERMINAL_BG,
                bold: false,
                italic: false,
                underline: false,
                strikeout: false,
            }]
        );
    }
    model.feed(b"hello\r\nworld");
    let lines = model.lines();
    assert_eq!(lines.len(), 3);
    assert_eq!(text(&lines[0]), "hello   ");
    assert_eq!(text(&lines[1]), "world   ");
    assert_eq!(text(&lines[2]), "        ");
    assert_eq!(lines[0].spans.len(), 1);
    assert_eq!(model.cursor(), Some((1, 5)));
}

#[test]
fn named_indexed_and_true_colors_resolve_for_foreground_and_background() {
    for index in 0_u8..16 {
        let fg_code = if index < 8 {
            30 + index
        } else {
            90 + index - 8
        };
        let bg_code = if index < 8 {
            40 + index
        } else {
            100 + index - 8
        };
        let span = first_span(format!("\x1b[{fg_code};{bg_code}mX").as_bytes());
        assert_eq!(span.fg, theme::ANSI[index as usize]);
        assert_eq!(span.bg, theme::ANSI[index as usize]);
    }
    for index in [0_u8, 7, 8, 15, 16, 196, 231, 232, 255] {
        let span = first_span(format!("\x1b[38;5;{index};48;5;{index}mX").as_bytes());
        assert_eq!(span.fg, theme::indexed_color(index));
        assert_eq!(span.bg, theme::indexed_color(index));
    }
    let span = first_span(b"\x1b[38;2;17;34;51;48;2;68;85;102mX");
    assert_eq!((span.fg, span.bg), (0x112233, 0x445566));
}

#[test]
fn bold_brightens_each_normal_named_color() {
    for index in 0..8 {
        let span = first_span(format!("\x1b[1;{}mX", 30 + index).as_bytes());
        assert!(span.bold);
        assert_eq!(span.fg, theme::ANSI[index + 8]);
    }
    let span = first_span(b"\x1b[1;38;2;17;34;51mX");
    assert!(span.bold);
    assert_eq!(span.fg, 0x112233);
}

#[test]
fn inverse_dim_and_hidden_resolve_colors() {
    let inverse = first_span(b"\x1b[38;2;90;120;150;48;2;30;60;90;7mX");
    assert_eq!((inverse.fg, inverse.bg), (0x1e3c5a, 0x5a7896));
    let dim = first_span(b"\x1b[38;2;91;121;151;48;2;30;60;90;2mX");
    assert_eq!((dim.fg, dim.bg), (0x466482, 0x1e3c5a));
    let hidden = first_span(b"\x1b[38;2;90;120;150;48;2;30;60;90;8mX");
    assert_eq!((hidden.fg, hidden.bg), (0x1e3c5a, 0x1e3c5a));
    assert!(hidden.text.starts_with('X'));
}

#[test]
fn adjacent_cells_are_grouped_by_the_complete_style() {
    let mut model = TerminalModel::new(8, 1);
    model.feed(b"ab\x1b[1;3;4;9mcd\x1b[0mef");
    assert_eq!(
        model.lines()[0].spans,
        vec![
            Span {
                text: "ab".into(),
                fg: theme::TERMINAL_FG,
                bg: theme::TERMINAL_BG,
                bold: false,
                italic: false,
                underline: false,
                strikeout: false
            },
            Span {
                text: "cd".into(),
                fg: theme::TERMINAL_FG,
                bg: theme::TERMINAL_BG,
                bold: true,
                italic: true,
                underline: true,
                strikeout: true
            },
            Span {
                text: "ef  ".into(),
                fg: theme::TERMINAL_FG,
                bg: theme::TERMINAL_BG,
                bold: false,
                italic: false,
                underline: false,
                strikeout: false
            },
        ]
    );
}

#[test]
fn wide_and_combining_characters_preserve_display_width_across_feed_chunks() {
    let mut model = TerminalModel::new(8, 2);
    let input = "A界e\u{301}Z";
    for byte in input.as_bytes() {
        model.feed(std::slice::from_ref(byte));
    }
    // A (1), the wide character (2), e plus its accent (1), Z (1), spaces (3).
    assert_eq!(text(&model.lines()[0]), "A界e\u{301}Z   ");
    assert_eq!(text(&model.lines()[1]), "        ");
    assert_eq!(model.cursor(), Some((0, 5)));
}

#[test]
fn cursor_visibility_and_terminal_key_modes_follow_private_mode_sequences() {
    let mut model = TerminalModel::new(10, 4);
    assert!(!model.key_modes().app_cursor);
    assert!(!model.key_modes().bracketed_paste);
    model.feed(b"\x1b[3;5H");
    assert_eq!(model.cursor(), Some((2, 4)));
    model.feed(b"\x1b[?25l");
    assert_eq!(model.cursor(), None);
    model.feed(b"\x1b[?25h\x1b[?1h");
    assert_eq!(model.cursor(), Some((2, 4)));
    assert!(model.key_modes().app_cursor);
    assert!(!model.key_modes().bracketed_paste);
    model.feed(b"\x1b[?2004h");
    assert!(model.key_modes().app_cursor);
    assert!(model.key_modes().bracketed_paste);
    model.feed(b"\x1b[?1l\x1b[?2004l");
    assert!(!model.key_modes().app_cursor);
    assert!(!model.key_modes().bracketed_paste);
}

#[test]
fn resize_changes_both_screen_dimensions_and_remains_feedable() {
    let mut model = TerminalModel::new(8, 3);
    for (cols, rows) in [(12, 5), (4, 2), (1, 1)] {
        model.resize(cols, rows);
        assert_eq!(model.size(), (cols, rows));
        model.feed(b"\x1b[2J\x1b[HX");
        let lines = model.lines();
        assert_eq!(lines.len(), usize::from(rows));
        assert_eq!(
            text(&lines[0]),
            format!("X{}", " ".repeat(usize::from(cols - 1)))
        );
        for line in lines {
            assert_eq!(text(&line).chars().count(), usize::from(cols));
        }
    }
}
