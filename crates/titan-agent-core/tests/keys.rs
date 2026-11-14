use titan_agent_core::keys::{encode_key, encode_paste, Key, KeyModes, Modifiers};

fn modifiers(bits: u8) -> Modifiers {
    Modifiers {
        shift: bits & 1 != 0,
        alt: bits & 2 != 0,
        ctrl: bits & 4 != 0,
    }
}

#[test]
fn characters_preserve_utf8_and_shift_and_support_alt() {
    for c in ['a', 'A', 'é', '界', '🦀'] {
        for shift in [false, true] {
            for alt in [false, true] {
                let mut expected = Vec::new();
                if alt {
                    expected.push(0x1b);
                }
                expected.extend(c.to_string().as_bytes());
                assert_eq!(
                    encode_key(
                        Key::Char(c),
                        Modifiers {
                            shift,
                            alt,
                            ctrl: false
                        },
                        KeyModes::default()
                    ),
                    Some(expected)
                );
            }
        }
    }
}

#[test]
fn control_characters_cover_letters_punctuation_and_unsupported_input() {
    let mut cases: Vec<(char, u8)> = ('a'..='z').zip(1..=26).collect();
    cases.extend(('A'..='Z').zip(1..=26));
    cases.extend([
        ('@', 0),
        (' ', 0),
        ('[', 27),
        ('\\', 28),
        (']', 29),
        ('^', 30),
        ('_', 31),
        ('?', 127),
    ]);
    for (c, byte) in cases {
        for bits in [4, 5, 6, 7] {
            let expected = if bits & 2 != 0 {
                vec![27, byte]
            } else {
                vec![byte]
            };
            assert_eq!(
                encode_key(Key::Char(c), modifiers(bits), KeyModes::default()),
                Some(expected)
            );
        }
    }
    for c in ['1', '!', 'é', '界'] {
        assert_eq!(
            encode_key(Key::Char(c), modifiers(4), KeyModes::default()),
            None
        );
        assert_eq!(
            encode_key(Key::Char(c), modifiers(6), KeyModes::default()),
            None
        );
    }
}

#[test]
fn basic_keys_apply_control_shift_and_alt_rules() {
    for bits in 0..8 {
        for (key, bytes) in [
            (Key::Enter, vec![13]),
            (Key::Backspace, vec![if bits & 4 != 0 { 8 } else { 127 }]),
            (
                Key::Tab,
                if bits & 1 != 0 {
                    b"\x1b[Z".to_vec()
                } else {
                    vec![9]
                },
            ),
            (Key::Escape, vec![27]),
        ] {
            let mut expected = Vec::new();
            if bits & 2 != 0 {
                expected.push(27);
            }
            expected.extend(bytes);
            assert_eq!(
                encode_key(key, modifiers(bits), KeyModes::default()),
                Some(expected)
            );
        }
    }
}

#[test]
fn cursor_keys_use_application_mode_only_without_modifiers() {
    for (key, suffix) in [
        (Key::Up, 'A'),
        (Key::Down, 'B'),
        (Key::Right, 'C'),
        (Key::Left, 'D'),
        (Key::Home, 'H'),
        (Key::End, 'F'),
    ] {
        for app_cursor in [false, true] {
            for bits in 0..8 {
                let expected = if bits == 0 {
                    format!("\x1b{}{suffix}", if app_cursor { 'O' } else { '[' })
                } else {
                    format!("\x1b[1;{}{suffix}", bits + 1)
                };
                assert_eq!(
                    encode_key(
                        key,
                        modifiers(bits),
                        KeyModes {
                            app_cursor,
                            bracketed_paste: false
                        }
                    ),
                    Some(expected.into_bytes())
                );
            }
        }
    }
}

#[test]
fn numbered_keys_and_function_keys_encode_all_modifier_combinations() {
    let mut numbered = vec![
        (Key::Insert, 2),
        (Key::Delete, 3),
        (Key::PageUp, 5),
        (Key::PageDown, 6),
    ];
    numbered.extend(
        (5..=12)
            .zip([15, 17, 18, 19, 20, 21, 23, 24])
            .map(|(f, n)| (Key::F(f), n)),
    );
    for bits in 0..8 {
        for &(key, number) in &numbered {
            let expected = if bits == 0 {
                format!("\x1b[{number}~")
            } else {
                format!("\x1b[{number};{}~", bits + 1)
            };
            assert_eq!(
                encode_key(key, modifiers(bits), KeyModes::default()),
                Some(expected.into_bytes())
            );
        }
        for (f, suffix) in (1..=4).zip(['P', 'Q', 'R', 'S']) {
            let expected = if bits == 0 {
                format!("\x1bO{suffix}")
            } else {
                format!("\x1b[1;{}{suffix}", bits + 1)
            };
            assert_eq!(
                encode_key(Key::F(f), modifiers(bits), KeyModes::default()),
                Some(expected.into_bytes())
            );
        }
        for f in [0, 13, 255] {
            assert_eq!(
                encode_key(Key::F(f), modifiers(bits), KeyModes::default()),
                None
            );
        }
    }
}

#[test]
fn paste_normalizes_plain_newlines_and_sanitizes_bracketed_terminators() {
    assert_eq!(encode_paste("", KeyModes::default()), b"");
    assert_eq!(
        encode_paste("é\nalpha\r\nbeta\rgamma", KeyModes::default()),
        "é\ralpha\rbeta\rgamma".as_bytes()
    );
    let modes = KeyModes {
        bracketed_paste: true,
        app_cursor: false,
    };
    assert_eq!(encode_paste("", modes), b"\x1b[200~\x1b[201~");
    assert_eq!(
        encode_paste("é\n\r\n\x1b[201~alpha\x1b[201~", modes),
        "\x1b[200~é\n\r\nalpha\x1b[201~".as_bytes()
    );
}
