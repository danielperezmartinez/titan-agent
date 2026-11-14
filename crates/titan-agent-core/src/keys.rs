#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    F(u8),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyModes {
    pub app_cursor: bool,
    pub bracketed_paste: bool,
}
fn m(x: Modifiers) -> u8 {
    1 + x.shift as u8 + 2 * x.alt as u8 + 4 * x.ctrl as u8
}
pub fn encode_key(k: Key, x: Modifiers, mode: KeyModes) -> Option<Vec<u8>> {
    let mut b = match k {
        Key::Char(c) => {
            if x.ctrl {
                let v = match c {
                    'a'..='z' => c as u8 - b'a' + 1,
                    'A'..='Z' => c as u8 - b'A' + 1,
                    '@' | ' ' | '[' | '\\' | ']' | '^' | '_' => match c {
                        '@' | ' ' => 0,
                        '[' => 27,
                        '\\' => 28,
                        ']' => 29,
                        '^' => 30,
                        _ => 31,
                    },
                    '?' => 127,
                    _ => return None,
                };
                vec![v]
            } else {
                c.to_string().into_bytes()
            }
        }
        Key::Enter => vec![13],
        Key::Backspace => vec![if x.ctrl { 8 } else { 127 }],
        Key::Tab if x.shift => b"\x1b[Z".to_vec(),
        Key::Tab => vec![9],
        Key::Escape => vec![27],
        _ => {
            let seq = match k {
                Key::Up => Some("A"),
                Key::Down => Some("B"),
                Key::Right => Some("C"),
                Key::Left => Some("D"),
                Key::Home => Some("H"),
                Key::End => Some("F"),
                _ => None,
            };
            if let Some(c) = seq {
                if m(x) == 1 {
                    format!("\x1b{}{}", if mode.app_cursor { "O" } else { "[" }, c).into_bytes()
                } else {
                    format!("\x1b[1;{}{}", m(x), c).into_bytes()
                }
            } else {
                let n = match k {
                    Key::Insert => 2,
                    Key::Delete => 3,
                    Key::PageUp => 5,
                    Key::PageDown => 6,
                    Key::F(5) => 15,
                    Key::F(6) => 17,
                    Key::F(7) => 18,
                    Key::F(8) => 19,
                    Key::F(9) => 20,
                    Key::F(10) => 21,
                    Key::F(11) => 23,
                    Key::F(12) => 24,
                    _ => 0,
                };
                if n > 0 {
                    if m(x) == 1 {
                        format!("\x1b[{n}~").into_bytes()
                    } else {
                        format!("\x1b[{n};{}~", m(x)).into_bytes()
                    }
                } else if let Key::F(f @ 1..=4) = k {
                    let c = ['P', 'Q', 'R', 'S'][(f - 1) as usize];
                    if m(x) == 1 {
                        format!("\x1bO{c}").into_bytes()
                    } else {
                        format!("\x1b[1;{}{c}", m(x)).into_bytes()
                    }
                } else {
                    return None;
                }
            }
        }
    };
    if x.alt
        && matches!(
            k,
            Key::Char(_) | Key::Enter | Key::Backspace | Key::Tab | Key::Escape
        )
    {
        b.insert(0, 27)
    }
    Some(b)
}
pub fn encode_paste(text: &str, modes: KeyModes) -> Vec<u8> {
    if modes.bracketed_paste {
        let clean = text.replace("\x1b[201~", "");
        format!("\x1b[200~{clean}\x1b[201~").into_bytes()
    } else {
        text.replace("\r\n", "\n").replace('\n', "\r").into_bytes()
    }
}
