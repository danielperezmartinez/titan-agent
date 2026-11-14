use titan_agent::keymap::key_from_gpui;
use titan_agent_core::keys::Key;

#[test]
fn every_named_key_takes_precedence_over_key_char() {
    for (name, expected) in [
        ("enter", Key::Enter),
        ("backspace", Key::Backspace),
        ("tab", Key::Tab),
        ("escape", Key::Escape),
        ("up", Key::Up),
        ("down", Key::Down),
        ("left", Key::Left),
        ("right", Key::Right),
        ("home", Key::Home),
        ("end", Key::End),
        ("pageup", Key::PageUp),
        ("pagedown", Key::PageDown),
        ("insert", Key::Insert),
        ("delete", Key::Delete),
        ("space", Key::Char(' ')),
    ] {
        assert_eq!(key_from_gpui(name, None), Some(expected), "{name}");
        assert_eq!(key_from_gpui(name, Some("z")), Some(expected), "{name}");
    }
    for number in 1..=12 {
        let name = format!("f{number}");
        assert_eq!(key_from_gpui(&name, None), Some(Key::F(number)));
        assert_eq!(key_from_gpui(&name, Some("z")), Some(Key::F(number)));
    }
}

#[test]
fn character_fallback_counts_unicode_characters_and_prefers_key_char() {
    for (key, key_char, expected) in [
        ("a", Some("A"), Some(Key::Char('A'))),
        ("unknown", Some("界"), Some(Key::Char('界'))),
        ("", Some("é"), Some(Key::Char('é'))),
        ("é", None, Some(Key::Char('é'))),
        ("界", None, Some(Key::Char('界'))),
        ("q", Some(""), Some(Key::Char('q'))),
        ("q", Some("ab"), Some(Key::Char('q'))),
        ("q", Some("e\u{301}"), Some(Key::Char('q'))),
        ("", None, None),
        ("unknown", Some(""), None),
        ("unknown", Some("ab"), None),
        ("unknown", Some("e\u{301}"), None),
    ] {
        assert_eq!(
            key_from_gpui(key, key_char),
            expected,
            "{key:?}, {key_char:?}"
        );
    }
}

#[test]
fn standalone_modifiers_and_unknown_names_are_not_terminal_keys() {
    for name in ["shift", "ctrl", "alt", "super", "platform", "f0", "f13"] {
        assert_eq!(key_from_gpui(name, None), None, "{name}");
    }
}
