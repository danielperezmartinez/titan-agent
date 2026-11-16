//! The editing model of the brief field: text, selection and IME marked
//! range, without GPUI types, so its rules can be tested on their own.

use std::ops::Range;

/// Text and selection, without any GPUI type, so the editing rules can be
/// tested on their own. Offsets are UTF-8 byte offsets on char boundaries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BriefText {
    pub text: String,
    pub selection: Range<usize>,
    pub reversed: bool,
    pub marked: Option<Range<usize>>,
}

impl BriefText {
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

    fn text(s: &str, cursor: usize) -> BriefText {
        BriefText {
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
}
