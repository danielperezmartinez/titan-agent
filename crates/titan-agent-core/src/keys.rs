#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyMods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
}

fn modified(mods: KeyMods) -> bool {
    mods.ctrl || mods.alt || mods.shift
}
fn prefix_alt(mut bytes: Vec<u8>, alt: bool) -> Vec<u8> {
    if alt {
        bytes.insert(0, 0x1b);
    }
    bytes
}

pub fn encode_key(
    key: &str,
    key_char: Option<&str>,
    mods: KeyMods,
    app_cursor: bool,
) -> Option<Vec<u8>> {
    if mods.platform {
        return None;
    }
    let code = match key {
        "enter" => {
            return Some(if mods.alt || mods.shift {
                b"\x1b\r".to_vec()
            } else {
                b"\r".to_vec()
            });
        }
        "tab" => {
            return Some(if mods.shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            });
        }
        "escape" => return Some(vec![0x1b]),
        "backspace" => {
            return Some(prefix_alt(
                vec![if mods.ctrl { 0x08 } else { 0x7f }],
                mods.alt,
            ));
        }
        "space" => return Some(prefix_alt(vec![if mods.ctrl { 0 } else { b' ' }], mods.alt)),
        _ => None,
    };
    if let Some(bytes) = code {
        return Some(bytes);
    }
    let cursor = match key {
        "up" => Some('A'),
        "down" => Some('B'),
        "right" => Some('C'),
        "left" => Some('D'),
        "home" => Some('H'),
        "end" => Some('F'),
        _ => None,
    };
    if let Some(suffix) = cursor {
        return Some(if modified(mods) {
            format!(
                "\x1b[1;{}{suffix}",
                1 + u8::from(mods.shift) + 2 * u8::from(mods.alt) + 4 * u8::from(mods.ctrl)
            )
            .into_bytes()
        } else if app_cursor {
            format!("\x1bO{suffix}").into_bytes()
        } else {
            format!("\x1b[{suffix}").into_bytes()
        });
    }
    let tilde = match key {
        "insert" => Some(2),
        "delete" => Some(3),
        "pageup" => Some(5),
        "pagedown" => Some(6),
        "f5" => Some(15),
        "f6" => Some(17),
        "f7" => Some(18),
        "f8" => Some(19),
        "f9" => Some(20),
        "f10" => Some(21),
        "f11" => Some(23),
        "f12" => Some(24),
        _ => None,
    };
    if let Some(number) = tilde {
        return Some(if modified(mods) {
            format!(
                "\x1b[{number};{}~",
                1 + u8::from(mods.shift) + 2 * u8::from(mods.alt) + 4 * u8::from(mods.ctrl)
            )
            .into_bytes()
        } else {
            format!("\x1b[{number}~").into_bytes()
        });
    }
    if let Some(suffix) = match key {
        "f1" => Some('P'),
        "f2" => Some('Q'),
        "f3" => Some('R'),
        "f4" => Some('S'),
        _ => None,
    } {
        return Some(if modified(mods) {
            format!(
                "\x1b[1;{}{suffix}",
                1 + u8::from(mods.shift) + 2 * u8::from(mods.alt) + 4 * u8::from(mods.ctrl)
            )
            .into_bytes()
        } else {
            format!("\x1bO{suffix}").into_bytes()
        });
    }
    if mods.ctrl {
        let character = key.chars().next()?;
        if key.chars().count() != 1 {
            return None;
        }
        let byte = match character.to_ascii_lowercase() {
            'a'..='z' => (character.to_ascii_lowercase() as u8) & 0x1f,
            '@' | '2' => 0,
            '[' => 0x1b,
            '\\' => 0x1c,
            ']' => 0x1d,
            '^' | '6' => 0x1e,
            '_' | '-' | '/' => 0x1f,
            _ => return None,
        };
        return Some(prefix_alt(vec![byte], mods.alt));
    }
    if mods.alt
        && let Some(text) = key_char
    {
        return Some(prefix_alt(text.as_bytes().to_vec(), true));
    }
    if let Some(text) = key_char {
        return Some(text.as_bytes().to_vec());
    }
    let mut chars = key.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        Some(first.to_string().into_bytes())
    } else {
        None
    }
}

pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        let sanitized: String = normalized.chars().filter(|c| *c != '\x1b').collect();
        format!("\x1b[200~{sanitized}\x1b[201~").into_bytes()
    } else {
        normalized.into_bytes()
    }
}
