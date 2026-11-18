//! The editing model of the common text field (docs/Decisiones visuales/
//! Conversación de sesión y campo de texto común.md): text, selection, IME
//! marked range, what each editing key does, the field's height and a draft
//! per conversation. Without a window, so its rules can be tested on their
//! own; the GPUI component (`app/composer.rs`) only paints and routes keys.
//!
//! Nothing here completes or rewrites what the user types (ADR-0002): the
//! only text it adds is the `- ` bullet the user asks for with Shift+Enter
//! on a list item.

use std::collections::HashMap;
use std::ops::Range;

use gpui::Keystroke;

/// Visible lines of the field: it grows with the text up to the maximum
/// and then scrolls.
pub const MIN_ROWS: usize = 2;
pub const MAX_ROWS: usize = 8;

/// The editing keys the model handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKey {
    Enter,
    ShiftEnter,
    Tab,
    ShiftTab,
    Up,
    Escape,
}

/// What a key press asks of the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// The key did nothing (the text and the selection are unchanged).
    Nothing,
    /// The text or the selection changed.
    Edited,
    /// Enter: the view launches (or sends) this text. The model does not
    /// clear it.
    Submit(String),
    /// Escape without a selection: the field gives up the focus.
    Blur,
    /// Not the model's: the view handles the key itself (the text is
    /// unchanged).
    Unhandled,
}

/// The editing key of a keystroke, if it is one.
pub fn edit_key(keystroke: &Keystroke) -> Option<EditKey> {
    let m = &keystroke.modifiers;
    let none = !m.control && !m.alt && !m.shift && !m.platform;
    let shift_only = m.shift && !m.control && !m.alt && !m.platform;
    match keystroke.key.as_str() {
        // ctrl+enter was the old way to launch: it still launches.
        "enter" if !m.shift && !m.alt => Some(EditKey::Enter),
        "enter" if shift_only => Some(EditKey::ShiftEnter),
        "tab" if none => Some(EditKey::Tab),
        "tab" if shift_only => Some(EditKey::ShiftTab),
        "up" if none => Some(EditKey::Up),
        "escape" if none => Some(EditKey::Escape),
        _ => None,
    }
}

/// Visible lines for a text of `visual_lines` lines.
pub fn rows_shown(visual_lines: usize) -> usize {
    visual_lines.clamp(MIN_ROWS, MAX_ROWS)
}

/// Pasted text with its line breaks kept as `\n`.
pub fn normalize_paste(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Length of the `<tabs>- ` prefix of a list item line, or `None` when the
/// line is not one.
fn list_prefix(line: &str) -> Option<usize> {
    let tabs = line.len() - line.trim_start_matches('\t').len();
    line[tabs..].starts_with("- ").then_some(tabs + 2)
}

/// A draft per conversation (session key, or the new session's own key)
/// and the last text sent from it, for ↑.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drafts {
    drafts: HashMap<String, String>,
    sent: HashMap<String, String>,
}

impl Drafts {
    pub fn save(&mut self, key: &str, text: &str) {
        if text.is_empty() {
            self.drafts.remove(key);
        } else {
            self.drafts.insert(key.to_string(), text.to_string());
        }
    }

    pub fn draft(&self, key: &str) -> &str {
        self.drafts.get(key).map_or("", String::as_str)
    }

    pub fn record_sent(&mut self, key: &str, text: &str) {
        self.sent.insert(key.to_string(), text.to_string());
        self.drafts.remove(key);
    }

    pub fn last_sent(&self, key: &str) -> Option<&str> {
        self.sent.get(key).map(String::as_str)
    }
}

/// Text and selection, without any GPUI type, so the editing rules can be
/// tested on their own. Offsets are UTF-8 byte offsets on char boundaries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposerText {
    pub text: String,
    pub selection: Range<usize>,
    pub reversed: bool,
    pub marked: Option<Range<usize>>,
}

impl ComposerText {
    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    pub fn move_to(&mut self, offset: usize) {
        let offset = offset.min(self.text.len());
        self.selection = offset..offset;
        self.reversed = false;
    }

    pub fn select_to(&mut self, offset: usize) {
        let offset = offset.min(self.text.len());
        if self.reversed {
            self.selection.start = offset;
        } else {
            self.selection.end = offset;
        }
        if self.selection.end < self.selection.start {
            self.reversed = !self.reversed;
            self.selection = self.selection.end..self.selection.start;
        }
    }

    /// Moves or extends the selection to `offset`.
    pub fn go(&mut self, offset: usize, extend: bool) {
        if extend {
            self.select_to(offset);
        } else {
            self.move_to(offset);
        }
    }

    pub fn select_all(&mut self) {
        self.selection = 0..self.text.len();
        self.reversed = false;
    }

    /// Replaces `range` (or the selection) with `new`, leaving the cursor
    /// after it.
    pub fn replace(&mut self, range: Option<Range<usize>>, new: &str) {
        let range = range
            .or(self.marked.clone())
            .unwrap_or(self.selection.clone());
        self.text.replace_range(range.clone(), new);
        let end = range.start + new.len();
        self.selection = end..end;
        self.reversed = false;
        self.marked = None;
    }

    pub fn previous_boundary(&self, offset: usize) -> usize {
        self.text[..offset]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    pub fn next_boundary(&self, offset: usize) -> usize {
        self.text[offset..]
            .chars()
            .next()
            .map_or(self.text.len(), |c| offset + c.len_utf8())
    }

    /// Start of the previous word (ctrl+left).
    pub fn previous_word(&self, offset: usize) -> usize {
        let before = &self.text[..offset];
        let trimmed = before.trim_end_matches(|c: char| !c.is_alphanumeric());
        trimmed
            .char_indices()
            .rev()
            .find(|(_, c)| !c.is_alphanumeric())
            .map_or(0, |(i, c)| i + c.len_utf8())
    }

    /// End of the next word (ctrl+right).
    pub fn next_word(&self, offset: usize) -> usize {
        let after = &self.text[offset..];
        let skip = after.len()
            - after
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .len();
        let rest = &after[skip..];
        let word = rest
            .char_indices()
            .find(|(_, c)| !c.is_alphanumeric())
            .map_or(rest.len(), |(i, _)| i);
        offset + skip + word
    }

    /// Start of the hard line that contains `offset`.
    pub fn line_start(&self, offset: usize) -> usize {
        self.text[..offset].rfind('\n').map_or(0, |i| i + 1)
    }

    /// End of the hard line that contains `offset` (before its newline).
    pub fn line_end(&self, offset: usize) -> usize {
        self.text[offset..]
            .find('\n')
            .map_or(self.text.len(), |i| offset + i)
    }

    /// Deletes the selection or, with an empty one, the char before (or
    /// after, with `forward`) the cursor.
    pub fn delete(&mut self, forward: bool) {
        if self.selection.is_empty() {
            let cursor = self.cursor();
            let other = if forward {
                self.next_boundary(cursor)
            } else {
                self.previous_boundary(cursor)
            };
            if other == cursor {
                return;
            }
            self.selection = cursor.min(other)..cursor.max(other);
        }
        self.replace(None, "");
    }

    /// What an editing key does (see [`EditKey`] and [`Effect`]).
    pub fn press(&mut self, key: EditKey, last_sent: Option<&str>) -> Effect {
        if key == EditKey::Enter {
            if self.marked.is_some() {
                return Effect::Unhandled;
            }
            if self.text.trim().is_empty() {
                return Effect::Nothing;
            }
            return Effect::Submit(self.text.clone());
        }
        let collapsed = self.selection.is_empty();
        match key {
            EditKey::Enter => unreachable!("handled above"),
            EditKey::ShiftEnter => {
                self.marked = None;
                if !collapsed {
                    self.replace(None, "");
                }
                let c = self.cursor();
                let (start, end) = (self.line_start(c), self.line_end(c));
                let line = &self.text[start..end];
                match list_prefix(line) {
                    // An empty item ends the list: its bullet goes away.
                    Some(prefix) if prefix == line.len() && c == end => {
                        self.replace(Some(start..end), "");
                    }
                    Some(prefix) if c >= start + prefix => {
                        let bullet = format!("\n{}- ", &line[..prefix - 2]);
                        self.replace(Some(c..c), &bullet);
                    }
                    _ => self.replace(Some(c..c), "\n"),
                }
                Effect::Edited
            }
            EditKey::Tab => {
                self.marked = None;
                let c = self.cursor();
                let start = self.line_start(c);
                if collapsed && list_prefix(&self.text[start..self.line_end(c)]).is_some() {
                    self.text.insert(start, '\t');
                    self.move_to(c + 1);
                } else {
                    self.replace(None, "\t");
                }
                Effect::Edited
            }
            EditKey::ShiftTab => {
                let c = self.cursor();
                let start = self.line_start(c);
                let line = &self.text[start..self.line_end(c)];
                if !collapsed || list_prefix(line).is_none() || !line.starts_with('\t') {
                    return Effect::Nothing;
                }
                self.text.remove(start);
                self.marked = None;
                self.move_to(c.saturating_sub(1).max(start));
                Effect::Edited
            }
            EditKey::Up => match last_sent.filter(|s| !s.is_empty()) {
                Some(last) if self.text.is_empty() => {
                    self.text = last.to_string();
                    self.marked = None;
                    self.move_to(self.text.len());
                    Effect::Edited
                }
                _ => Effect::Unhandled,
            },
            EditKey::Escape => {
                if collapsed {
                    return Effect::Blur;
                }
                self.move_to(self.cursor());
                self.marked = None;
                Effect::Edited
            }
        }
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.clone()]
    }

    pub fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for c in self.text.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += c.len_utf16();
            utf8 += c.len_utf8();
        }
        utf8
    }

    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())]
            .chars()
            .map(char::len_utf16)
            .sum()
    }

    pub fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    pub fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str, cursor: usize) -> ComposerText {
        ComposerText {
            text: s.into(),
            selection: cursor..cursor,
            ..Default::default()
        }
    }

    #[test]
    fn typing_and_deleting_respect_char_boundaries() {
        let mut t = text("añb", 3);
        t.delete(false);
        assert_eq!(t.text, "ab");
        assert_eq!(t.cursor(), 1);
        t.replace(None, "ñ\n");
        assert_eq!(t.text, "añ\nb");
        t.delete(true);
        assert_eq!(t.text, "añ\n");
        t.move_to(0);
        t.delete(false);
        assert_eq!(t.text, "añ\n");
    }

    #[test]
    fn selection_extends_both_ways() {
        let mut t = text("hola mundo", 4);
        t.select_to(0);
        assert_eq!((t.selection.clone(), t.reversed), (0..4, true));
        t.select_to(10);
        assert_eq!((t.selection.clone(), t.reversed), (4..10, false));
        assert_eq!(t.selected_text(), " mundo");
        t.replace(None, "!");
        assert_eq!(t.text, "hola!");
    }

    #[test]
    fn words_and_lines() {
        let t = text("uno  dos-tres\ncuatro", 0);
        assert_eq!(t.next_word(0), 3);
        assert_eq!(t.next_word(3), 8);
        assert_eq!(t.previous_word(8), 5);
        assert_eq!(t.previous_word(5), 0);
        assert_eq!(t.line_start(16), 14);
        assert_eq!(t.line_end(2), 13);
        assert_eq!(t.line_end(15), t.text.len());
    }

    #[test]
    fn utf16_round_trip() {
        let t = text("a😀b", 0);
        assert_eq!(t.offset_to_utf16(5), 3);
        assert_eq!(t.offset_from_utf16(3), 5);
        assert_eq!(t.range_from_utf16(&(1..3)), 1..5);
    }

    #[test]
    fn lists_continue_indent_and_end() {
        let mut t = text("Encargo:\n- añade", 0);
        t.move_to(t.text.len());
        assert_eq!(t.press(EditKey::ShiftEnter, None), Effect::Edited);
        assert_eq!(t.text, "Encargo:\n- añade\n- ");
        assert_eq!(t.press(EditKey::Tab, None), Effect::Edited);
        assert_eq!(t.text, "Encargo:\n- añade\n\t- ");
        assert_eq!(t.cursor(), t.text.len());
        assert_eq!(t.press(EditKey::ShiftTab, None), Effect::Edited);
        assert_eq!(t.press(EditKey::ShiftTab, None), Effect::Nothing);
        assert_eq!(t.press(EditKey::ShiftEnter, None), Effect::Edited);
        assert_eq!(t.text, "Encargo:\n- añade\n");
        assert_eq!(t.press(EditKey::ShiftEnter, None), Effect::Edited);
        assert_eq!(t.text, "Encargo:\n- añade\n\n");
        // Before the bullet, a plain line break.
        let mut t = text("- uno", 1);
        t.press(EditKey::ShiftEnter, None);
        assert_eq!(t.text, "-\n uno");
    }

    #[test]
    fn enter_up_and_escape() {
        let mut t = text("  \n", 0);
        assert_eq!(t.press(EditKey::Enter, None), Effect::Nothing);
        let mut t = text("", 0);
        assert_eq!(t.press(EditKey::Up, Some("")), Effect::Unhandled);
        assert_eq!(t.press(EditKey::Up, Some("hola ñ")), Effect::Edited);
        assert_eq!((t.text.as_str(), t.cursor()), ("hola ñ", 7));
        assert_eq!(t.press(EditKey::Up, Some("otro")), Effect::Unhandled);
        assert_eq!(t.press(EditKey::Enter, None), Effect::Submit("hola ñ".into()));
        t.marked = Some(0..1);
        assert_eq!(t.press(EditKey::Enter, None), Effect::Unhandled);
        t.marked = None;
        t.select_to(0);
        assert_eq!(t.press(EditKey::Escape, None), Effect::Edited);
        assert_eq!(t.selection, 0..0);
        assert_eq!(t.press(EditKey::Escape, None), Effect::Blur);
    }

    #[test]
    fn keys_sizes_paste_and_drafts() {
        let key = |key: &str, shift, control, alt| Keystroke {
            key: key.into(),
            modifiers: gpui::Modifiers {
                shift,
                control,
                alt,
                ..Default::default()
            },
            key_char: None,
        };
        assert_eq!(edit_key(&key("enter", false, true, false)), Some(EditKey::Enter));
        assert_eq!(edit_key(&key("enter", true, true, false)), None);
        assert_eq!(edit_key(&key("enter", false, false, true)), None);
        assert_eq!(edit_key(&key("up", true, false, false)), None);
        assert_eq!([0, 1, 5, 9].map(rows_shown), [2, 2, 5, 8]);
        assert_eq!(normalize_paste("a\r\nb\rc\td"), "a\nb\nc\td");
        let mut drafts = Drafts::default();
        drafts.save("a", "uno");
        drafts.save("b", "dos");
        drafts.record_sent("a", "uno");
        assert_eq!((drafts.draft("a"), drafts.draft("b")), ("", "dos"));
        assert_eq!((drafts.last_sent("a"), drafts.last_sent("b")), (Some("uno"), None));
        drafts.save("b", "");
        assert_eq!(drafts.draft("b"), "");
    }
}
