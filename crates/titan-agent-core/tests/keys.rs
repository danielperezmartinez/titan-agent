use titan_agent_core::keys::{KeyMods, encode_key, encode_paste};

fn modifiers(bits: u8) -> KeyMods {
    KeyMods {
        shift: bits & 1 != 0,
        alt: bits & 2 != 0,
        ctrl: bits & 4 != 0,
        platform: false,
    }
}

fn assert_key(key: &str, text: Option<&str>, mods: KeyMods, app: bool, expected: &[u8]) {
    assert_eq!(
        encode_key(key, text, mods, app),
        Some(expected.to_vec()),
        "key={key:?}, text={text:?}, mods={mods:?}, app={app}"
    );
}

#[test]
fn default_modifiers_are_all_false() {
    assert_eq!(modifiers(0), KeyMods::default());
}

#[test]
fn platform_shortcuts_never_send_terminal_bytes() {
    for bits in 0..8 {
        let mods = KeyMods {
            platform: true,
            ..modifiers(bits)
        };
        for key in ["a", "enter", "tab", "up", "delete", "f1", "space", "escape"] {
            for app in [false, true] {
                assert_eq!(encode_key(key, Some("a"), mods, app), None);
            }
        }
    }
}

#[test]
fn special_keys_have_the_declared_bytes() {
    for (key, bits, expected) in [
        ("enter", 0, &b"\r"[..]),
        ("enter", 4, &b"\r"[..]),
        ("enter", 1, &b"\x1b\r"[..]),
        ("enter", 2, &b"\x1b\r"[..]),
        ("enter", 3, &b"\x1b\r"[..]),
        ("enter", 5, &b"\x1b\r"[..]),
        ("enter", 6, &b"\x1b\r"[..]),
        ("enter", 7, &b"\x1b\r"[..]),
        ("tab", 0, &b"\t"[..]),
        ("tab", 1, &b"\x1b[Z"[..]),
        ("escape", 0, &b"\x1b"[..]),
        ("backspace", 0, &b"\x7f"[..]),
        ("backspace", 4, &b"\x08"[..]),
        ("backspace", 2, &b"\x1b\x7f"[..]),
        ("space", 0, &b" "[..]),
        ("space", 4, &b"\x00"[..]),
        ("space", 2, &b"\x1b "[..]),
    ] {
        for app in [false, true] {
            assert_key(key, None, modifiers(bits), app, expected);
        }
    }
}

#[test]
fn cursor_keys_obey_application_mode_and_all_modifier_combinations() {
    for (key, suffix) in [
        ("up", 'A'),
        ("down", 'B'),
        ("right", 'C'),
        ("left", 'D'),
        ("home", 'H'),
        ("end", 'F'),
    ] {
        for app in [false, true] {
            for bits in 0..8 {
                let expected = if bits != 0 {
                    format!("\x1b[1;{}{suffix}", 1 + bits)
                } else if app {
                    format!("\x1bO{suffix}")
                } else {
                    format!("\x1b[{suffix}")
                };
                assert_key(key, None, modifiers(bits), app, expected.as_bytes());
            }
        }
    }
}

#[test]
fn tilde_keys_and_function_keys_encode_all_modifiers() {
    for (key, number) in [
        ("insert", 2),
        ("delete", 3),
        ("pageup", 5),
        ("pagedown", 6),
        ("f5", 15),
        ("f6", 17),
        ("f7", 18),
        ("f8", 19),
        ("f9", 20),
        ("f10", 21),
        ("f11", 23),
        ("f12", 24),
    ] {
        for bits in 0..8 {
            let expected = if bits == 0 {
                format!("\x1b[{number}~")
            } else {
                format!("\x1b[{number};{}~", 1 + bits)
            };
            for app in [false, true] {
                assert_key(key, None, modifiers(bits), app, expected.as_bytes());
            }
        }
    }
    for (key, suffix) in [("f1", 'P'), ("f2", 'Q'), ("f3", 'R'), ("f4", 'S')] {
        for bits in 0..8 {
            let expected = if bits == 0 {
                format!("\x1bO{suffix}")
            } else {
                format!("\x1b[1;{}{suffix}", 1 + bits)
            };
            for app in [false, true] {
                assert_key(key, None, modifiers(bits), app, expected.as_bytes());
            }
        }
    }
}

#[test]
fn control_letters_use_key_identity_with_optional_shift_and_alt() {
    for letter in b'a'..=b'z' {
        let key = char::from(letter).to_string();
        for bits in [4, 5, 6, 7] {
            let text = if bits & 1 != 0 {
                key.to_uppercase()
            } else {
                key.clone()
            };
            let mut expected = Vec::new();
            if bits & 2 != 0 {
                expected.push(0x1b);
            }
            expected.push(letter & 0x1f);
            assert_key(&key, Some(&text), modifiers(bits), false, &expected);
            assert_key(&key, None, modifiers(bits), true, &expected);
        }
    }
}

#[test]
fn control_punctuation_and_digits_have_explicit_mappings() {
    for (key, byte) in [
        ("@", 0),
        ("2", 0),
        ("[", 0x1b),
        ("\\", 0x1c),
        ("]", 0x1d),
        ("^", 0x1e),
        ("6", 0x1e),
        ("_", 0x1f),
        ("-", 0x1f),
        ("/", 0x1f),
    ] {
        for bits in [4, 6] {
            let expected = if bits == 6 {
                vec![0x1b, byte]
            } else {
                vec![byte]
            };
            assert_key(key, Some(key), modifiers(bits), false, &expected);
        }
    }
    for key in ["1", "3", "?", "ñ", "€", "unknown"] {
        for bits in [4, 5, 6, 7] {
            assert_eq!(encode_key(key, Some(key), modifiers(bits), false), None);
        }
    }
}

#[test]
fn text_preserves_utf8_and_alt_prefixes_the_produced_text() {
    for (key, text, bits) in [
        ("a", "a", 0),
        ("a", "A", 1),
        ("n", "ñ", 0),
        ("e", "€", 0),
        ("unknown", "two", 0),
        ("a", "A", 3),
        ("n", "ñ", 2),
        ("e", "€", 2),
    ] {
        let mut expected = Vec::new();
        if bits & 2 != 0 {
            expected.push(0x1b);
        }
        expected.extend_from_slice(text.as_bytes());
        assert_key(key, Some(text), modifiers(bits), false, &expected);
    }
    for key in ["a", "ñ", "€", "🦀"] {
        assert_key(key, None, KeyMods::default(), false, key.as_bytes());
    }
    for key in [
        "", "shift", "ctrl", "alt", "capslock", "unknown", "f13", "ab",
    ] {
        assert_eq!(encode_key(key, None, KeyMods::default(), false), None);
    }
}

#[test]
fn paste_normalizes_newlines_and_preserves_other_text() {
    for (input, normalized) in [
        ("", ""),
        ("plain ñ €", "plain ñ €"),
        ("a\r\nb\nc\rd\n\n", "a\rb\rc\rd\r\r"),
        ("\r\n\n\r", "\r\r\r"),
        ("a\x1b[31m\tb\0", "a\x1b[31m\tb\0"),
    ] {
        assert_eq!(encode_paste(input, false), normalized.as_bytes());
        let sanitized = normalized.replace('\x1b', "");
        let expected = format!("\x1b[200~{sanitized}\x1b[201~");
        assert_eq!(encode_paste(input, true), expected.as_bytes());
    }
}

#[test]
fn bracketed_paste_removes_embedded_escape_characters_including_end_markers() {
    let text = "\x1b\x1b[201~payload\x1b[200~\x1b";
    assert_eq!(encode_paste(text, false), text.as_bytes());
    assert_eq!(
        encode_paste(text, true),
        b"\x1b[200~[201~payload[200~\x1b[201~"
    );
}
