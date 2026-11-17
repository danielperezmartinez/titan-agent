//! Translation of GPUI keystrokes into the bytes an xterm-compatible terminal
//! expects.
//!
//! Only special keys and control or alt combinations are sent from the key
//! handler. Typed text is left to the platform input handler, which delivers
//! it already composed (dead keys, compose sequences and IME), as the
//! terminal of Zed does: a dead key arrives as a keystroke with no
//! `key_char`, and sending its key would write the wrong character.

use gpui::Keystroke;

/// Terminal modes that change what some keys send.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyModes {
    /// DECCKM: arrows send `ESC O x` instead of `ESC [ x`.
    pub app_cursor: bool,
}

/// Returns the bytes for a keystroke, or `None` when the terminal should not
/// receive it (for example a lone modifier).
pub fn keystroke_bytes(keystroke: &Keystroke, modes: KeyModes) -> Option<Vec<u8>> {
    let m = &keystroke.modifiers;
    let key = keystroke.key.as_str();
    // xterm modifier parameter: 1 + shift + 2*alt + 4*ctrl.
    let modifier = 1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.control as u8;
    let csi_letter = |letter: char| -> Vec<u8> {
        if modifier > 1 {
            format!("\x1b[1;{modifier}{letter}").into_bytes()
        } else if modes.app_cursor {
            format!("\x1bO{letter}").into_bytes()
        } else {
            format!("\x1b[{letter}").into_bytes()
        }
    };
    let csi_tilde = |code: u8| -> Vec<u8> {
        if modifier > 1 {
            format!("\x1b[{code};{modifier}~").into_bytes()
        } else {
            format!("\x1b[{code}~").into_bytes()
        }
    };
    let bytes = match key {
        "enter" => {
            if m.alt {
                b"\x1b\r".to_vec()
            } else {
                b"\r".to_vec()
            }
        }
        "tab" if m.shift => b"\x1b[Z".to_vec(),
        "tab" => b"\t".to_vec(),
        "escape" => b"\x1b".to_vec(),
        "backspace" => {
            if m.control {
                b"\x08".to_vec()
            } else if m.alt {
                b"\x1b\x7f".to_vec()
            } else {
                b"\x7f".to_vec()
            }
        }
        "up" => csi_letter('A'),
        "down" => csi_letter('B'),
        "right" => csi_letter('C'),
        "left" => csi_letter('D'),
        "home" => csi_letter('H'),
        "end" => csi_letter('F'),
        "insert" => csi_tilde(2),
        "delete" => csi_tilde(3),
        "pageup" => csi_tilde(5),
        "pagedown" => csi_tilde(6),
        "f1" => b"\x1bOP".to_vec(),
        "f2" => b"\x1bOQ".to_vec(),
        "f3" => b"\x1bOR".to_vec(),
        "f4" => b"\x1bOS".to_vec(),
        "f5" => csi_tilde(15),
        "f6" => csi_tilde(17),
        "f7" => csi_tilde(18),
        "f8" => csi_tilde(19),
        "f9" => csi_tilde(20),
        "f10" => csi_tilde(21),
        "f11" => csi_tilde(23),
        "f12" => csi_tilde(24),
        "space" if m.control => {
            if m.alt {
                vec![0x1b, 0]
            } else {
                vec![0]
            }
        }
        "space" if m.alt && !m.platform => b"\x1b ".to_vec(),
        "space" => return None,
        // Windows reports AltGr as control + alt: the character it gives is
        // text, not a control code.
        _ if m.control && m.alt && is_alt_text(keystroke) => return None,
        _ if m.control && !m.platform => {
            let c = key.chars().next()?;
            if key.chars().count() != 1 {
                return None;
            }
            let code = match c.to_ascii_lowercase() {
                c @ 'a'..='z' => c as u8 - b'a' + 1,
                '@' | '2' => 0,
                '[' | '3' => 0x1b,
                '\\' | '4' => 0x1c,
                ']' | '5' => 0x1d,
                '^' | '6' => 0x1e,
                '_' | '-' | '7' => 0x1f,
                '?' | '8' => 0x7f,
                _ => return None,
            };
            if m.alt { vec![0x1b, code] } else { vec![code] }
        }
        // Alt as meta: ESC and the key, when alt does not give another
        // character.
        _ if m.alt && !m.platform && keystroke.key_char.as_deref() == Some(key) => {
            let mut bytes = vec![0x1b];
            bytes.extend(key.as_bytes());
            bytes
        }
        // Typed text (and dead keys, window shortcuts, lone modifiers): the
        // input handler delivers the text, already composed.
        _ => return None,
    };
    Some(bytes)
}

/// Whether the keystroke gives a character other than its key (alt or AltGr
/// text, such as `@` on a Spanish `2`).
fn is_alt_text(keystroke: &Keystroke) -> bool {
    keystroke
        .key_char
        .as_deref()
        .is_some_and(|text| text != keystroke.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(keystroke: &'static str, app_cursor: bool) -> Option<Vec<u8>> {
        let keystroke = Keystroke::parse(keystroke).unwrap();
        keystroke_bytes(&keystroke, KeyModes { app_cursor })
    }

    #[test]
    fn control_keys_send_c0_codes() {
        assert_eq!(bytes("ctrl-c", false), Some(vec![3]));
        assert_eq!(bytes("ctrl-u", false), Some(vec![0x15]));
        assert_eq!(bytes("ctrl-[", false), Some(vec![0x1b]));
        assert_eq!(bytes("ctrl-space", false), Some(vec![0]));
        assert_eq!(bytes("ctrl-alt-a", false), Some(vec![0x1b, 1]));
    }

    #[test]
    fn editing_keys_follow_xterm() {
        assert_eq!(bytes("enter", false), Some(b"\r".to_vec()));
        assert_eq!(bytes("backspace", false), Some(vec![0x7f]));
        assert_eq!(bytes("tab", false), Some(b"\t".to_vec()));
        assert_eq!(bytes("shift-tab", false), Some(b"\x1b[Z".to_vec()));
        assert_eq!(bytes("escape", false), Some(b"\x1b".to_vec()));
        assert_eq!(bytes("delete", false), Some(b"\x1b[3~".to_vec()));
        assert_eq!(bytes("pageup", false), Some(b"\x1b[5~".to_vec()));
        assert_eq!(bytes("f5", false), Some(b"\x1b[15~".to_vec()));
    }

    #[test]
    fn arrows_honour_application_cursor_mode_and_modifiers() {
        assert_eq!(bytes("up", false), Some(b"\x1b[A".to_vec()));
        assert_eq!(bytes("up", true), Some(b"\x1bOA".to_vec()));
        assert_eq!(bytes("ctrl-left", true), Some(b"\x1b[1;5D".to_vec()));
        assert_eq!(bytes("shift-end", false), Some(b"\x1b[1;2F".to_vec()));
    }

    #[test]
    fn typed_text_is_left_to_the_input_handler() {
        let mut keystroke = Keystroke::parse("a").unwrap();
        keystroke.key_char = Some("a".into());
        assert_eq!(keystroke_bytes(&keystroke, KeyModes::default()), None);
        let mut accented = Keystroke::parse("e").unwrap();
        accented.key_char = Some("é".into());
        assert_eq!(keystroke_bytes(&accented, KeyModes::default()), None);
        // A dead key: no character yet, and its key is not what it writes.
        assert_eq!(bytes("'", false), None, "dead keys send nothing");
        assert_eq!(bytes("space", false), None, "space is text too");
        assert_eq!(bytes("shift", false), None, "lone modifiers send nothing");
    }

    #[test]
    fn alt_sends_escape_only_for_its_own_key() {
        let mut meta = Keystroke::parse("alt-b").unwrap();
        meta.key_char = Some("b".into());
        assert_eq!(
            keystroke_bytes(&meta, KeyModes::default()),
            Some(b"\x1bb".to_vec())
        );
        let mut altgr = Keystroke::parse("ctrl-alt-2").unwrap();
        altgr.key_char = Some("@".into());
        assert_eq!(keystroke_bytes(&altgr, KeyModes::default()), None);
        assert_eq!(bytes("ctrl-alt-space", false), Some(vec![0x1b, 0]));
        assert_eq!(bytes("alt-space", false), Some(b"\x1b ".to_vec()));
    }
}
