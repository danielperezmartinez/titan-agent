//! Acceptance tests for the pure demo parsers; no window is opened.
use gpui::{Keystroke, Modifiers};
use std::time::Duration;
use titan_agent::demo::{DemoStep, parse_keys, parse_size};

fn key(name: &str, text: Option<&str>, control: bool, alt: bool, shift: bool) -> DemoStep {
    DemoStep::Key(Keystroke {
        key: name.into(),
        key_char: text.map(Into::into),
        modifiers: Modifiers {
            control,
            alt,
            shift,
            platform: false,
            function: false,
        },
    })
}

#[test]
fn c4_size_accepts_inclusive_bounds_and_rejects_other_forms() {
    for (input, expected) in [
        ("1100x800", (1100, 800)),
        ("640x480", (640, 480)),
        ("7680x4320", (7680, 4320)),
        ("640x4320", (640, 4320)),
        ("7680x480", (7680, 480)),
    ] {
        assert_eq!(parse_size(input), Ok(expected), "{input:?}");
    }
    for input in [
        "639x800",
        "7681x800",
        "1100x479",
        "1100x4321",
        "0x0",
        "",
        "1100",
        "1100X800",
        " 1100x800",
        "1100x800 ",
        "1100 x800",
        "1100x 800",
        "1100x800x2",
        "-1100x800",
        "+1100x800",
        "1100x-800",
        "axb",
        "1100.0x800",
        "1100x",
        "x800",
        "999999999999999999999x800",
    ] {
        assert!(parse_size(input).is_err(), "accepted {input:?}");
    }
}

#[test]
fn c4_keys_preserve_text_modifiers_literals_pauses_and_step_order() {
    assert_eq!(parse_keys("").unwrap(), Vec::<DemoStep>::new());
    let steps =
        parse_keys("aA ñÁ-{{}}{enter}{shift-enter}{ctrl-n}{alt-shift-up}{pause 0}z{pause 10000}")
            .unwrap();
    let expected = vec![
        key("a", Some("a"), false, false, false),
        key("a", Some("A"), false, false, true),
        key("space", Some(" "), false, false, false),
        key("ñ", Some("ñ"), false, false, false),
        key("á", Some("Á"), false, false, true),
        key("-", Some("-"), false, false, false),
        key("{", Some("{"), false, false, false),
        key("}", Some("}"), false, false, false),
        key("enter", None, false, false, false),
        key("enter", None, false, false, true),
        key("n", None, true, false, false),
        key("up", None, false, true, true),
        DemoStep::Pause(Duration::from_millis(0)),
        key("z", Some("z"), false, false, false),
        DemoStep::Pause(Duration::from_millis(10000)),
    ];
    assert_eq!(steps, expected);
    // The public enum also promises Clone, Debug and PartialEq.
    assert_eq!(steps.clone(), steps);
    for name in [
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
    ] {
        assert_eq!(
            parse_keys(&format!("{{{name}}}")).unwrap(),
            vec![key(name, None, false, false, false)]
        );
    }
    for modifiers in [
        "ctrl-alt-shift-",
        "ctrl-shift-alt-",
        "alt-ctrl-shift-",
        "alt-shift-ctrl-",
        "shift-ctrl-alt-",
        "shift-alt-ctrl-",
    ] {
        assert_eq!(
            parse_keys(&format!("{{{modifiers}n}}")).unwrap(),
            vec![key("n", None, true, true, true)]
        );
    }
    // Named characters are lowercased; only explicit modifiers apply inside braces.
    for (input, name) in [("{A}", "a"), ("{Á}", "á"), ("{ctrl--}", "-")] {
        assert_eq!(
            parse_keys(input).unwrap(),
            vec![key(name, None, input == "{ctrl--}", false, false)]
        );
    }
    assert_eq!(
        parse_keys("{pause 125}").unwrap(),
        vec![DemoStep::Pause(Duration::from_millis(125))]
    );
}

#[test]
fn c4_invalid_keys_report_the_offending_fragment() {
    for (input, fragment) in [
        ("prefix{", "{"),
        ("prefix}", "}"),
        ("{f13}", "f13"),
        ("{meta-a}", "meta-a"),
        ("{}", "{}"),
        ("{ctrl-ctrl-n}", "ctrl-ctrl-n"),
        ("{alt-alt-up}", "alt-alt-up"),
        ("{shift-shift-enter}", "shift-shift-enter"),
        ("{pause 10001}", "pause 10001"),
        ("{pause -1}", "pause -1"),
        ("{pause kite}", "pause kite"),
        (
            "{pause 999999999999999999999}",
            "pause 999999999999999999999",
        ),
    ] {
        let error = parse_keys(input).expect_err(input);
        assert!(
            error.contains(fragment),
            "{input:?}: error does not name {fragment:?}: {error}"
        );
    }
    for input in ["a\nb", "a\rb", "a\tb", "{pause  1}", "{pause\t1}"] {
        // The contract does not specify how whitespace fragments are escaped in diagnostics.
        let error = parse_keys(input).expect_err(input);
        assert!(!error.is_empty(), "{input:?}");
    }
}
