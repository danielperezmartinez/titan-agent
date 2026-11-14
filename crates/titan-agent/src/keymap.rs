use titan_agent_core::keys::Key;
pub fn key_from_gpui(key: &str, key_char: Option<&str>) -> Option<Key> {
    Some(match key {
        "enter" => Key::Enter,
        "backspace" => Key::Backspace,
        "tab" => Key::Tab,
        "escape" => Key::Escape,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "insert" => Key::Insert,
        "delete" => Key::Delete,
        "space" => Key::Char(' '),
        "f1" => Key::F(1),
        "f2" => Key::F(2),
        "f3" => Key::F(3),
        "f4" => Key::F(4),
        "f5" => Key::F(5),
        "f6" => Key::F(6),
        "f7" => Key::F(7),
        "f8" => Key::F(8),
        "f9" => Key::F(9),
        "f10" => Key::F(10),
        "f11" => Key::F(11),
        "f12" => Key::F(12),
        _ => return character(key, key_char),
    })
}
fn character(key: &str, key_char: Option<&str>) -> Option<Key> {
    key_char
        .and_then(|s| {
            let mut c = s.chars();
            let x = c.next()?;
            c.next().is_none().then_some(Key::Char(x))
        })
        .or_else(|| {
            let mut c = key.chars();
            let x = c.next()?;
            c.next().is_none().then_some(Key::Char(x))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_function_key_names_are_special() {
        for name in ["foo", "f01", "f13"] {
            assert_eq!(key_from_gpui(name, Some("ñ")), Some(Key::Char('ñ')));
        }
        assert_eq!(key_from_gpui("f12", Some("x")), Some(Key::F(12)));
    }
}
