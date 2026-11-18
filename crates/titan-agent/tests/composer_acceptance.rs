use gpui::{Keystroke, Modifiers};
use titan_agent::composer::{
    ComposerText, Drafts, EditKey, Effect, MAX_ROWS, MIN_ROWS, edit_key, normalize_paste,
    rows_shown,
};

#[test]
fn key_mapping_obeys_the_modifier_table_and_ignores_function_and_key_char() {
    // Exhaust all control/alt/shift/platform combinations from the contract.
    for key in ["enter", "tab", "up", "escape", "a"] {
        for mask in 0..16 {
            let control = mask & 1 != 0;
            let alt = mask & 2 != 0;
            let shift = mask & 4 != 0;
            let platform = mask & 8 != 0;
            let expected = match key {
                "enter" if !alt && !shift => Some(EditKey::Enter),
                "enter" if mask == 4 => Some(EditKey::ShiftEnter),
                "tab" if mask == 0 => Some(EditKey::Tab),
                "tab" if mask == 4 => Some(EditKey::ShiftTab),
                "up" if mask == 0 => Some(EditKey::Up),
                "escape" if mask == 0 => Some(EditKey::Escape),
                _ => None,
            };
            for function in [false, true] {
                for key_char in [None, Some("unrelated".into())] {
                    let stroke = Keystroke {
                        key: key.into(),
                        key_char,
                        modifiers: Modifiers {
                            control,
                            alt,
                            shift,
                            platform,
                            function,
                        },
                    };
                    assert_eq!(
                        edit_key(&stroke),
                        expected,
                        "key={key}, mask={mask}, function={function}"
                    );
                }
            }
        }
    }
}

fn text(value: &str) -> ComposerText {
    ComposerText {
        text: value.into(),
        selection: value.len()..value.len(),
        reversed: false,
        marked: None,
    }
}

fn edited(model: &ComposerText, expected: &str, cursor: usize) {
    assert_eq!(model.text, expected);
    assert_eq!(model.selection, cursor..cursor);
    assert_eq!(model.cursor(), cursor);
    assert!(!model.reversed);
    assert_eq!(model.marked, None);
}

#[test]
fn submit_recall_and_escape_preserve_user_text_and_respect_ime() {
    let mut model = text("  encargo café\n\tfin  ");
    model.selection = 2..9;
    model.reversed = true;
    let before = model.clone();
    assert_eq!(
        model.press(EditKey::Enter, None),
        Effect::Submit(before.text.clone())
    );
    assert_eq!(model, before);
    model.marked = Some(2..9);
    let composing = model.clone();
    assert_eq!(model.press(EditKey::Enter, None), Effect::Unhandled);
    assert_eq!(model, composing);
    assert_eq!(model.press(EditKey::Escape, None), Effect::Edited);
    edited(&model, &before.text, 2);
    let before_blur = model.clone();
    assert_eq!(model.press(EditKey::Escape, None), Effect::Blur);
    assert_eq!(model, before_blur);
    model.selection = 2..9;
    assert_eq!(model.press(EditKey::Escape, None), Effect::Edited);
    edited(&model, &before.text, 9);

    for value in ["", " \n\t "] {
        let mut blank = text(value);
        let before = blank.clone();
        assert_eq!(blank.press(EditKey::Enter, None), Effect::Nothing);
        assert_eq!(blank, before);
    }
    let mut empty = text("");
    for last in [None, Some("")] {
        assert_eq!(empty.press(EditKey::Up, last), Effect::Unhandled);
        assert_eq!(empty, text(""));
    }
    assert_eq!(
        empty.press(EditKey::Up, Some("café\nacción")),
        Effect::Edited
    );
    edited(&empty, "café\nacción", "café\nacción".len());
    let recalled = empty.clone();
    assert_eq!(empty.press(EditKey::Up, Some("other")), Effect::Unhandled);
    assert_eq!(empty, recalled);
}

#[test]
fn shift_enter_continues_indented_lists_ends_empty_items_and_replaces_selection() {
    let mut model = text("intro\n\t- café");
    model.marked = Some(0..5);
    assert_eq!(model.press(EditKey::ShiftEnter, None), Effect::Edited);
    edited(
        &model,
        "intro\n\t- café\n\t- ",
        "intro\n\t- café\n\t- ".len(),
    );
    assert_eq!(model.press(EditKey::ShiftEnter, None), Effect::Edited);
    edited(&model, "intro\n\t- café\n", "intro\n\t- café\n".len());

    let mut split = text("\t- café fin");
    split.move_to("\t- café".len());
    assert_eq!(split.press(EditKey::ShiftEnter, None), Effect::Edited);
    edited(&split, "\t- café\n\t-  fin", "\t- café\n\t- ".len());
    let mut before_bullet_end = text("\t- café");
    before_bullet_end.move_to(1);
    assert_eq!(
        before_bullet_end.press(EditKey::ShiftEnter, None),
        Effect::Edited
    );
    edited(&before_bullet_end, "\t\n- café", 2);

    let mut selection = text("\t- café");
    selection.selection = 3..selection.text.len();
    selection.reversed = true;
    assert_eq!(selection.press(EditKey::ShiftEnter, None), Effect::Edited);
    edited(&selection, "", 0);
    let mut plain = text("café fin");
    plain.selection = "café".len()..plain.text.len();
    assert_eq!(plain.press(EditKey::ShiftEnter, None), Effect::Edited);
    edited(&plain, "café\n", "café\n".len());
}

#[test]
fn tabs_indent_only_unselected_list_items_and_otherwise_insert_literal_tabs() {
    let mut model = text("intro\n- café");
    assert_eq!(model.press(EditKey::Tab, None), Effect::Edited);
    edited(&model, "intro\n\t- café", "intro\n\t- café".len());
    assert_eq!(model.press(EditKey::ShiftTab, None), Effect::Edited);
    edited(&model, "intro\n- café", "intro\n- café".len());
    let before = model.clone();
    assert_eq!(model.press(EditKey::ShiftTab, None), Effect::Nothing);
    assert_eq!(model, before);
    let mut at_start = text("intro\n\t\t- café");
    at_start.move_to("intro\n".len());
    assert_eq!(at_start.press(EditKey::ShiftTab, None), Effect::Edited);
    edited(&at_start, "intro\n\t- café", "intro\n".len());

    let mut selected = text("\t- café");
    selected.selection = 3..selected.text.len();
    selected.reversed = true;
    selected.marked = Some(3..selected.text.len());
    let before = selected.clone();
    assert_eq!(selected.press(EditKey::ShiftTab, None), Effect::Nothing);
    assert_eq!(selected, before);
    assert_eq!(selected.press(EditKey::Tab, None), Effect::Edited);
    edited(&selected, "\t- \t", 4);
    let mut plain = text("café");
    assert_eq!(plain.press(EditKey::Tab, None), Effect::Edited);
    edited(&plain, "café\t", "café\t".len());
    let before = plain.clone();
    assert_eq!(plain.press(EditKey::ShiftTab, None), Effect::Nothing);
    assert_eq!(plain, before);
}

#[test]
fn sizing_paste_and_conversation_drafts_preserve_content_and_isolate_keys() {
    assert_eq!((MIN_ROWS, MAX_ROWS), (2, 8));
    for (lines, expected) in [
        (0, 2),
        (1, 2),
        (2, 2),
        (5, 5),
        (8, 8),
        (9, 8),
        (usize::MAX, 8),
    ] {
        assert_eq!(rows_shown(lines), expected);
    }
    assert_eq!(
        normalize_paste("  café\r\n\t- acción\r\r\nfin\n "),
        "  café\n\t- acción\n\nfin\n "
    );
    assert_eq!(normalize_paste(""), "");
    let mut drafts = Drafts::default();
    for key in ["draft:new", "cl:example"] {
        assert_eq!(drafts.draft(key), "");
        assert_eq!(drafts.last_sent(key), None);
    }
    drafts.save("draft:new", "café\n\t- idea");
    drafts.save("cl:example", "acción");
    assert_eq!(drafts.draft("draft:new"), "café\n\t- idea");
    assert_eq!(drafts.draft("cl:example"), "acción");
    drafts.record_sent("draft:new", "sent café");
    assert_eq!(drafts.draft("draft:new"), "");
    assert_eq!(drafts.last_sent("draft:new"), Some("sent café"));
    assert_eq!(drafts.last_sent("cl:example"), None);
    assert_eq!(drafts.draft("cl:example"), "acción");
    drafts.save("draft:new", "revised");
    drafts.save("draft:new", "latest");
    assert_eq!(drafts.draft("draft:new"), "latest");
    assert_eq!(drafts.last_sent("draft:new"), Some("sent café"));
    drafts.save("draft:new", "");
    assert_eq!(drafts.draft("draft:new"), "");
    drafts.record_sent("cl:example", "sent acción");
    assert_eq!(drafts.draft("cl:example"), "");
    assert_eq!(drafts.last_sent("cl:example"), Some("sent acción"));
    drafts.record_sent("draft:new", "");
    assert_eq!(drafts.last_sent("draft:new"), Some(""));
    assert_eq!(drafts.last_sent("cl:example"), Some("sent acción"));
}
