use gpui::{Keystroke, Modifiers};
use titan_agent::keys::{keystroke_bytes, KeyModes};

fn key(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
    Keystroke {
        key: key.into(),
        key_char: key_char.map(Into::into),
        modifiers,
    }
}

fn modifiers(control: bool, alt: bool, shift: bool, platform: bool) -> Modifiers {
    Modifiers {
        control,
        alt,
        shift,
        platform,
        function: false,
    }
}

fn assert_bytes(event: Keystroke, modes: KeyModes, expected: Option<&[u8]>) {
    assert_eq!(
        keystroke_bytes(&event, modes).as_deref(),
        expected,
        "key={:?}, key_char={:?}, modifiers={:?}, modes={:?}",
        event.key,
        event.key_char,
        event.modifiers,
        modes,
    );
}

#[test]
fn linux_dead_key_sequences_leave_composed_text_to_the_input_handler() {
    let plain = modifiers(false, false, false, false);
    let shift = modifiers(false, false, true, false);
    let sequences = [
        vec![key("'", None, plain), key("e", Some("é"), plain)],
        vec![key("'", None, plain), key("acute", Some("´"), plain)],
        vec![key("ñ", Some("ñ"), plain)],
        vec![key("'", None, shift), key("u", Some("ü"), plain)],
    ];

    for sequence in sequences {
        for event in sequence {
            assert_bytes(event, KeyModes::default(), None);
        }
    }
}

#[test]
fn ordinary_text_window_shortcuts_and_modifier_keys_emit_nothing() {
    for shift in [false, true] {
        for (name, text) in [
            ("a", Some("a")),
            ("a", Some("A")),
            ("/", Some("?")),
            ("!", Some("!")),
            ("space", Some(" ")),
            ("space", None),
            ("'", None),
            ("`", None),
            ("i", Some("í")),
            ("2", Some("@")), // Linux AltGr has neither control nor alt set.
        ] {
            assert_bytes(
                key(name, text, modifiers(false, false, shift, false)),
                KeyModes::default(),
                None,
            );
        }
    }

    for control in [false, true] {
        for alt in [false, true] {
            assert_bytes(
                key("a", Some("a"), modifiers(control, alt, false, true)),
                KeyModes::default(),
                None,
            );
        }
    }

    for name in ["shift", "control", "alt", "platform", "function"] {
        assert_bytes(
            key(name, None, Modifiers::default()),
            KeyModes::default(),
            None,
        );
    }
}

#[test]
fn control_letters_and_punctuation_keep_their_terminal_codes() {
    let mut cases: Vec<(String, u8)> = ('a'..='z')
        .enumerate()
        .map(|(index, letter)| (letter.to_string(), index as u8 + 1))
        .collect();
    cases.extend(
        [
            ("space", 0),
            ("@", 0),
            ("2", 0),
            ("[", 0x1b),
            ("3", 0x1b),
            ("\\", 0x1c),
            ("4", 0x1c),
            ("]", 0x1d),
            ("5", 0x1d),
            ("^", 0x1e),
            ("6", 0x1e),
            ("_", 0x1f),
            ("-", 0x1f),
            ("7", 0x1f),
            ("?", 0x7f),
            ("8", 0x7f),
        ]
        .into_iter()
        .map(|(name, byte)| (name.to_owned(), byte)),
    );

    for alt in [false, true] {
        for (name, byte) in &cases {
            let expected = if alt { vec![0x1b, *byte] } else { vec![*byte] };
            assert_bytes(
                key(name, None, modifiers(true, alt, false, false)),
                KeyModes::default(),
                Some(&expected),
            );
        }
        assert_bytes(
            key("9", None, modifiers(true, alt, false, false)),
            KeyModes::default(),
            None,
        );
    }
}

#[test]
fn alt_meta_is_distinguished_from_alt_text_and_windows_altgr() {
    for (name, text, expected) in [
        ("b", Some("b"), Some(&b"\x1bb"[..])),
        ("ñ", Some("ñ"), Some("\x1bñ".as_bytes())),
        ("space", Some(" "), Some(&b"\x1b "[..])),
        ("b", None, None),
        ("e", Some("é"), None),
        ("2", Some("@"), None),
    ] {
        assert_bytes(
            key(name, text, modifiers(false, true, false, false)),
            KeyModes::default(),
            expected,
        );
    }

    // Apply the literal AltGr rule whenever key_char differs from key.
    for (name, text) in [("2", "@"), ("e", "€"), ("q", "@"), ("8", "[")] {
        assert_bytes(
            key(name, Some(text), modifiers(true, true, false, false)),
            KeyModes::default(),
            None,
        );
    }
    assert_bytes(
        key("b", Some("b"), modifiers(true, true, false, false)),
        KeyModes::default(),
        Some(b"\x1b\x02"),
    );
}

#[test]
fn special_keys_preserve_terminal_sequences_and_cursor_modes() {
    for (name, expected) in [
        ("enter", &b"\r"[..]),
        ("tab", &b"\t"[..]),
        ("escape", &b"\x1b"[..]),
        ("backspace", &b"\x7f"[..]),
        ("f1", &b"\x1bOP"[..]),
        ("f2", &b"\x1bOQ"[..]),
        ("f3", &b"\x1bOR"[..]),
        ("f4", &b"\x1bOS"[..]),
        ("f5", &b"\x1b[15~"[..]),
        ("f6", &b"\x1b[17~"[..]),
        ("f7", &b"\x1b[18~"[..]),
        ("f8", &b"\x1b[19~"[..]),
        ("f9", &b"\x1b[20~"[..]),
        ("f10", &b"\x1b[21~"[..]),
        ("f11", &b"\x1b[23~"[..]),
        ("f12", &b"\x1b[24~"[..]),
    ] {
        assert_bytes(
            key(name, None, Modifiers::default()),
            KeyModes::default(),
            Some(expected),
        );
    }
    for (name, mods, expected) in [
        (
            "enter",
            modifiers(false, true, false, false),
            &b"\x1b\r"[..],
        ),
        ("tab", modifiers(false, false, true, false), &b"\x1b[Z"[..]),
        (
            "backspace",
            modifiers(true, false, false, false),
            &b"\x08"[..],
        ),
        (
            "backspace",
            modifiers(false, true, false, false),
            &b"\x1b\x7f"[..],
        ),
    ] {
        assert_bytes(key(name, None, mods), KeyModes::default(), Some(expected));
    }

    for app_cursor in [false, true] {
        for shift in [false, true] {
            for alt in [false, true] {
                for control in [false, true] {
                    let mods = modifiers(control, alt, shift, false);
                    let parameter = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(control);
                    for (name, suffix) in [
                        ("up", 'A'),
                        ("down", 'B'),
                        ("right", 'C'),
                        ("left", 'D'),
                        ("home", 'H'),
                        ("end", 'F'),
                    ] {
                        let expected = if parameter != 1 {
                            format!("\x1b[1;{parameter}{suffix}")
                        } else if app_cursor {
                            format!("\x1bO{suffix}")
                        } else {
                            format!("\x1b[{suffix}")
                        };
                        assert_bytes(
                            key(name, None, mods),
                            KeyModes { app_cursor },
                            Some(expected.as_bytes()),
                        );
                    }
                    for (name, number) in
                        [("insert", 2), ("delete", 3), ("pageup", 5), ("pagedown", 6)]
                    {
                        let expected = if parameter == 1 {
                            format!("\x1b[{number}~")
                        } else {
                            format!("\x1b[{number};{parameter}~")
                        };
                        assert_bytes(
                            key(name, None, mods),
                            KeyModes { app_cursor },
                            Some(expected.as_bytes()),
                        );
                    }
                }
            }
        }
    }
}
