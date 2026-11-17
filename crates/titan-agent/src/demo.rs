//! Demo-only variables of the window, for the demo kit (scripts/demo/):
//! `TITAN_AGENT_DEMO_SIZE` (initial window size) and `TITAN_AGENT_DEMO_KEYS`
//! (keys applied to the window after it opens). Parsing lives here, without
//! a window, so it is tested; `main.rs` only applies the result.

use std::time::Duration;

use gpui::{Keystroke, Modifiers};

/// One step of a key script.
#[derive(Debug, Clone, PartialEq)]
pub enum DemoStep {
    /// A keystroke dispatched to the window as if it were pressed.
    Key(Keystroke),
    /// Wait before the next step.
    Pause(Duration),
}

const WIDTH: std::ops::RangeInclusive<u32> = 640..=7680;
const HEIGHT: std::ops::RangeInclusive<u32> = 480..=4320;
const MAX_PAUSE_MS: u64 = 10_000;
const NAMED_KEYS: [&str; 14] = [
    "enter",
    "tab",
    "escape",
    "backspace",
    "delete",
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pageup",
    "pagedown",
    "space",
];

/// Parses `<width>x<height>` in logical pixels, within 640..=7680 by
/// 480..=4320.
pub fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let bad = || format!("tamaño no válido: «{text}» (se espera ANCHOxALTO, p. ej. 1100x800)");
    let (w, h) = text.split_once('x').ok_or_else(bad)?;
    let number = |part: &str| -> Result<u32, String> {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        part.parse().map_err(|_| bad())
    };
    let (w, h) = (number(w)?, number(h)?);
    if !WIDTH.contains(&w) || !HEIGHT.contains(&h) {
        return Err(format!(
            "tamaño fuera de rango: {w}x{h} (ancho {}..={}, alto {}..={})",
            WIDTH.start(),
            WIDTH.end(),
            HEIGHT.start(),
            HEIGHT.end()
        ));
    }
    Ok((w, h))
}

/// Parses a key script: plain text is typed character by character,
/// `{[ctrl-][alt-][shift-]key}` presses a named key or a single character,
/// `{pause N}` waits N ms (0..=10000), and `{{` / `}}` type a brace.
pub fn parse_keys(script: &str) -> Result<Vec<DemoStep>, String> {
    let mut steps = Vec::new();
    let mut chars = script.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                steps.push(DemoStep::Key(typed('{')));
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                steps.push(DemoStep::Key(typed('}')));
            }
            '}' => return Err("llave «}» suelta (para escribirla, «}}»)".into()),
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => inner.push(c),
                        None => return Err(format!("llave sin cerrar: «{{{inner}»")),
                    }
                }
                steps.push(braced(&inner)?);
            }
            '\n' | '\r' | '\t' => {
                return Err(
                    "salto de línea o tabulador en el texto: usa {enter}, {shift-enter} o {tab}"
                        .into(),
                );
            }
            c => steps.push(DemoStep::Key(typed(c))),
        }
    }
    Ok(steps)
}

/// The keystroke that types one character.
fn typed(c: char) -> Keystroke {
    if c == ' ' {
        return Keystroke {
            modifiers: Modifiers::default(),
            key: "space".into(),
            key_char: Some(" ".into()),
        };
    }
    Keystroke {
        modifiers: Modifiers {
            shift: c.is_uppercase(),
            ..Modifiers::default()
        },
        key: c.to_lowercase().collect(),
        key_char: Some(c.to_string()),
    }
}

/// What goes between braces: a pause or a named key with modifiers.
fn braced(inner: &str) -> Result<DemoStep, String> {
    if let Some(text) = inner.strip_prefix("pause ") {
        let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
        return match text.parse::<u64>() {
            Ok(ms) if digits && ms <= MAX_PAUSE_MS => {
                Ok(DemoStep::Pause(Duration::from_millis(ms)))
            }
            _ => Err(format!(
                "pausa no válida: «{{{inner}}}» (0..={MAX_PAUSE_MS} ms)"
            )),
        };
    }
    let unknown = || format!("tecla desconocida: «{{{inner}}}»");
    let mut modifiers = Modifiers::default();
    let mut rest = inner;
    loop {
        let flag = if let Some(r) = rest.strip_prefix("ctrl-") {
            rest = r;
            &mut modifiers.control
        } else if let Some(r) = rest.strip_prefix("alt-") {
            rest = r;
            &mut modifiers.alt
        } else if let Some(r) = rest.strip_prefix("shift-") {
            rest = r;
            &mut modifiers.shift
        } else {
            break;
        };
        if *flag {
            return Err(format!("modificador repetido: «{{{inner}}}»"));
        }
        *flag = true;
    }
    let mut chars = rest.chars();
    let single = matches!(
        (chars.next(), chars.next()),
        (Some(c), None) if !c.is_control() && !c.is_whitespace()
    );
    if !single && !NAMED_KEYS.contains(&rest) {
        return Err(unknown());
    }
    Ok(DemoStep::Key(Keystroke {
        modifiers,
        key: rest.to_lowercase(),
        key_char: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(step: &DemoStep) -> &Keystroke {
        match step {
            DemoStep::Key(k) => k,
            other => panic!("not a key: {other:?}"),
        }
    }

    #[test]
    fn a_dash_after_modifiers_is_the_key() {
        let steps = parse_keys("{ctrl--}").unwrap();
        assert_eq!(key(&steps[0]).key, "-");
        assert!(key(&steps[0]).modifiers.control);
        // A modifier name alone is a single-letter key only when it is one.
        assert!(parse_keys("{ctrl-}").is_err());
        assert!(parse_keys("{pause -1}").is_err());
        assert!(parse_keys("{pause +5}").is_err());
        assert!(parse_keys("{Enter}").is_err());
    }
}
