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
        _ if key.len() >= 2 && key.starts_with('f') => {
            let n = key[1..].parse::<u8>().ok()?;
            if (1..=12).contains(&n) {
                Key::F(n)
            } else {
                return character(key, key_char);
            }
        }
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
