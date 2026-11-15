//! Multi-line text field. GPUI has no text input of its own; this one follows
//! the `input` example of GPUI (same input handler, so dead keys and IME
//! work) and adds line wrapping, several lines and vertical scrolling.
//!
//! The text is never rewritten: what the user types (or pastes) is what
//! [`TextArea::text`] returns.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, KeyDownEvent,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    SharedString, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window, WrappedLine,
    div, fill, point, prelude::*, px, relative, size,
};

use crate::theme::{self, color, hsla};

/// What the field tells its owner.
pub enum TextAreaEvent {
    /// The text changed.
    Changed,
    /// `ctrl+enter`: the user asks to submit.
    Submit,
}

pub struct TextArea {
    focus: FocusHandle,
    text: String,
    placeholder: SharedString,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    selecting: bool,
    min_rows: usize,
    max_rows: usize,
    /// Visual rows of the last layout (wrapped lines included).
    rows: usize,
    /// First visual row shown.
    scroll: usize,
    layout: Option<Layout>,
}

/// The last painted layout, to map positions to offsets.
struct Layout {
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    /// Each logical line with its first byte and first visual row.
    lines: Vec<(usize, usize, WrappedLine)>,
}

impl EventEmitter<TextAreaEvent> for TextArea {}

impl Focusable for TextArea {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

const LINE_HEIGHT: f32 = 19.;
const FONT_SIZE: f32 = 13.;

impl TextArea {
    pub fn new(
        placeholder: &str,
        min_rows: usize,
        max_rows: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        TextArea {
            focus: cx.focus_handle(),
            text: String::new(),
            placeholder: SharedString::from(placeholder.to_string()),
            selected: 0..0,
            reversed: false,
            marked: None,
            selecting: false,
            min_rows,
            max_rows,
            rows: 1,
            scroll: 0,
            layout: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Empties the field (after a launch).
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.text.clear();
        self.selected = 0..0;
        self.reversed = false;
        self.marked = None;
        self.scroll = 0;
        cx.emit(TextAreaEvent::Changed);
        cx.notify();
    }

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.text.len());
        self.selected = offset..offset;
        self.reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.text.len());
        if self.reversed {
            self.selected.start = offset;
        } else {
            self.selected.end = offset;
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        cx.notify();
    }

    fn move_or_select(&mut self, offset: usize, select: bool, cx: &mut Context<Self>) {
        if select {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn replace(&mut self, range: Range<usize>, new_text: &str, cx: &mut Context<Self>) {
        let range = clamp_range(&self.text, range);
        self.text.replace_range(range.clone(), new_text);
        let end = range.start + new_text.len();
        self.selected = end..end;
        self.reversed = false;
        self.marked = None;
        cx.emit(TextAreaEvent::Changed);
        cx.notify();
    }

    fn insert(&mut self, new_text: &str, cx: &mut Context<Self>) {
        let range = self.marked.clone().unwrap_or(self.selected.clone());
        self.replace(range, new_text, cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
        let shift = m.shift;
        let word = m.control || m.alt;
        let cursor = self.cursor();
        match keystroke.key.as_str() {
            "enter" if m.control || m.platform => cx.emit(TextAreaEvent::Submit),
            "enter" => self.insert("\n", cx),
            "tab" => {}
            "backspace" => {
                if self.selected.is_empty() {
                    let start = if word {
                        previous_word(&self.text, cursor)
                    } else {
                        previous_char(&self.text, cursor)
                    };
                    self.replace(start..cursor, "", cx);
                } else {
                    self.replace(self.selected.clone(), "", cx);
                }
            }
            "delete" => {
                if self.selected.is_empty() {
                    let end = if word {
                        next_word(&self.text, cursor)
                    } else {
                        next_char(&self.text, cursor)
                    };
                    self.replace(cursor..end, "", cx);
                } else {
                    self.replace(self.selected.clone(), "", cx);
                }
            }
            "left" => {
                let target = if !shift && !self.selected.is_empty() {
                    self.selected.start
                } else if word {
                    previous_word(&self.text, cursor)
                } else {
                    previous_char(&self.text, cursor)
                };
                self.move_or_select(target, shift, cx);
            }
            "right" => {
                let target = if !shift && !self.selected.is_empty() {
                    self.selected.end
                } else if word {
                    next_word(&self.text, cursor)
                } else {
                    next_char(&self.text, cursor)
                };
                self.move_or_select(target, shift, cx);
            }
            "up" | "down" => {
                let rows = if keystroke.key == "up" { -1. } else { 1. };
                let target = self.vertical(cursor, rows).unwrap_or(if rows < 0. {
                    0
                } else {
                    self.text.len()
                });
                self.move_or_select(target, shift, cx);
            }
            "home" => {
                let target = if m.control {
                    0
                } else {
                    self.text[..cursor].rfind('\n').map_or(0, |i| i + 1)
                };
                self.move_or_select(target, shift, cx);
            }
            "end" => {
                let target = if m.control {
                    self.text.len()
                } else {
                    self.text[cursor..]
                        .find('\n')
                        .map_or(self.text.len(), |i| cursor + i)
                };
                self.move_or_select(target, shift, cx);
            }
            "a" if m.control => {
                self.selected = 0..self.text.len();
                self.reversed = false;
                cx.notify();
            }
            "c" if m.control => {
                if !self.selected.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.text[self.selected.clone()].to_string(),
                    ));
                }
            }
            "x" if m.control => {
                if !self.selected.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.text[self.selected.clone()].to_string(),
                    ));
                    self.replace(self.selected.clone(), "", cx);
                }
            }
            "v" if m.control => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.insert(&text.replace("\r\n", "\n"), cx);
                }
            }
            // Printable keys go to the input handler (dead keys and IME
            // included); other shortcuts bubble up to the window.
            _ => return,
        }
        let _ = window;
        cx.stop_propagation();
    }

    /// Offset one visual row above or below `offset`, at the same x.
    fn vertical(&self, offset: usize, rows: f32) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        let (position, _) = layout.position(offset)?;
        let target = point(position.x, position.y + layout.line_height * rows);
        if target.y < px(0.) {
            return None;
        }
        layout.index_at(target)
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        self.selecting = true;
        let Some(index) = self.index_for_mouse(event.position) else {
            self.move_to(self.text.len(), cx);
            return;
        };
        if event.modifiers.shift {
            self.select_to(index, cx);
        } else {
            self.move_to(index, cx);
        }
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting
            && let Some(index) = self.index_for_mouse(event.position)
        {
            self.select_to(index, cx);
        }
    }

    fn index_for_mouse(&self, position: Point<Pixels>) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        let local = position - layout.bounds.origin;
        let local = point(local.x, local.y + layout.line_height * self.scroll as f32);
        if local.y < px(0.) {
            return Some(0);
        }
        layout.index_at(local).or(Some(self.text.len()))
    }

    fn utf16_to_offset(&self, utf16: usize) -> usize {
        let mut offset = 0;
        let mut count = 0;
        for ch in self.text.chars() {
            if count >= utf16 {
                break;
            }
            count += ch.len_utf16();
            offset += ch.len_utf8();
        }
        offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())]
            .chars()
            .map(char::len_utf16)
            .sum()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.utf16_to_offset(range.start)..self.utf16_to_offset(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }
}

impl Layout {
    /// Position of a byte offset relative to the top of the text (not
    /// scrolled), and the index of its logical line.
    fn position(&self, offset: usize) -> Option<(Point<Pixels>, usize)> {
        let index = self
            .lines
            .iter()
            .rposition(|(start, _, _)| *start <= offset)?;
        let (start, row, line) = &self.lines[index];
        let local = line.position_for_index(offset - start, self.line_height)?;
        Some((
            point(local.x, local.y + self.line_height * *row as f32),
            index,
        ))
    }

    /// Byte offset closest to a point relative to the top of the text.
    fn index_at(&self, target: Point<Pixels>) -> Option<usize> {
        let row = (target.y / self.line_height).floor() as usize;
        let (start, first_row, line) = self
            .lines
            .iter()
            .rev()
            .find(|(_, first_row, _)| *first_row <= row)?;
        let rows = line.wrap_boundaries().len() + 1;
        if row >= first_row + rows {
            // Past the last row of the last line.
            return Some(start + line.len());
        }
        let local = point(
            target.x.max(px(0.)),
            self.line_height * (row - first_row) as f32 + self.line_height / 2.,
        );
        let index = match line.closest_index_for_position(local, self.line_height) {
            Ok(index) | Err(index) => index,
        };
        Some(start + index.min(line.len()))
    }
}

impl EntityInputHandler for TextArea {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = clamp_range(&self.text, self.range_from_utf16(&range_utf16));
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected),
            reversed: self.reversed,
        })
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected = clamp_range(&self.text, self.range_from_utf16(&range_utf16));
        self.reversed = false;
        cx.notify();
    }

    fn text_length_utf16(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.text.len()))
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|range| self.range_from_utf16(&range))
            .or(self.marked.clone())
            .unwrap_or(self.selected.clone());
        self.replace(range, new_text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = clamp_range(
            &self.text,
            range_utf16
                .map(|range| self.range_from_utf16(&range))
                .or(self.marked.clone())
                .unwrap_or(self.selected.clone()),
        );
        self.text.replace_range(range.clone(), new_text);
        self.marked = (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        let start16 = self.offset_to_utf16(range.start);
        let len16 = new_text.encode_utf16().count();
        self.selected = new_selected_range_utf16
            .map(|selected| {
                self.range_from_utf16(
                    &(start16 + selected.start.min(len16)..start16 + selected.end.min(len16)),
                )
            })
            .unwrap_or(range.start + new_text.len()..range.start + new_text.len());
        self.reversed = false;
        cx.emit(TextAreaEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let layout = self.layout.as_ref()?;
        let (start, _) = layout.position(range.start)?;
        let scroll = layout.line_height * self.scroll as f32;
        let origin = layout.bounds.origin + point(start.x, start.y - scroll);
        Some(Bounds::new(origin, size(px(8.), layout.line_height)))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let index = self.index_for_mouse(point)?;
        Some(self.offset_to_utf16(index))
    }
}

impl Render for TextArea {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("text-area")
            .track_focus(&self.focus)
            .key_context("TextArea")
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.selecting = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.selecting = false),
            )
            .on_mouse_move(cx.listener(Self::mouse_move))
            .cursor_text()
            .w_full()
            .p(px(12.))
            .text_size(px(FONT_SIZE))
            .line_height(px(LINE_HEIGHT))
            .child(TextAreaElement { area: cx.entity() })
    }
}

struct TextAreaElement {
    area: Entity<TextArea>,
}

impl IntoElement for TextAreaElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct Prepaint {
    /// Vertical scroll applied to the text.
    shift: Pixels,
    selection: Vec<Bounds<Pixels>>,
    cursor: Option<Bounds<Pixels>>,
}

impl Element for TextAreaElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let area = self.area.read(cx);
        let rows = area.rows.clamp(area.min_rows, area.max_rows);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (px(LINE_HEIGHT) * rows as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let line_height = px(LINE_HEIGHT);
        let style = window.text_style();
        let font = style.font();
        let area = self.area.read(cx);
        let empty = area.text.is_empty();
        let (shown, text_color) = if empty {
            (area.placeholder.to_string(), hsla(theme::MUTE))
        } else {
            (area.text.clone(), hsla(theme::BODY))
        };
        let run = |len: usize| TextRun {
            len,
            font: font.clone(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match (&area.marked, empty) {
            (Some(marked), false) => vec![
                run(marked.start),
                TextRun {
                    underline: Some(UnderlineStyle {
                        color: Some(text_color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..run(marked.end - marked.start)
                },
                run(shown.len() - marked.end),
            ],
            _ => vec![run(shown.len())],
        };
        let wrapped = window
            .text_system()
            .shape_text(
                SharedString::from(shown.clone()),
                px(FONT_SIZE),
                &runs,
                Some(bounds.size.width),
                None,
            )
            .unwrap_or_default();

        // Logical lines with their first byte and visual row.
        let mut lines = Vec::new();
        let mut offset = 0;
        let mut row = 0;
        for line in wrapped {
            let rows = line.wrap_boundaries().len() + 1;
            let len = line.len();
            lines.push((offset, row, line));
            offset += len + 1;
            row += rows;
        }
        let total_rows = row.max(1);
        let layout = Layout {
            bounds,
            line_height,
            lines,
        };

        let (cursor_offset, selected, focused) = (
            area.cursor(),
            area.selected.clone(),
            area.focus.is_focused(window),
        );
        let visible = area.rows.clamp(area.min_rows, area.max_rows);
        let mut scroll = area.scroll;
        let cursor_pos = if empty {
            Some((point(px(0.), px(0.)), 0))
        } else {
            layout.position(cursor_offset)
        };
        if let Some((pos, _)) = cursor_pos {
            let cursor_row = (pos.y / line_height).round() as usize;
            if cursor_row < scroll {
                scroll = cursor_row;
            } else if cursor_row >= scroll + visible {
                scroll = cursor_row + 1 - visible;
            }
        }
        scroll = scroll.min(total_rows.saturating_sub(visible));
        let shift = line_height * scroll as f32;

        let cursor = cursor_pos
            .filter(|_| focused && selected.is_empty())
            .map(|(pos, _)| {
                Bounds::new(
                    point(bounds.left() + pos.x, bounds.top() + pos.y - shift),
                    size(px(1.), line_height),
                )
            });
        let mut selection = Vec::new();
        if !empty
            && !selected.is_empty()
            && let (Some((start, _)), Some((end, _))) = (
                layout.position(selected.start),
                layout.position(selected.end),
            )
        {
            let top = |p: Point<Pixels>| bounds.top() + p.y - shift;
            if start.y == end.y {
                selection.push(Bounds::from_corners(
                    point(bounds.left() + start.x, top(start)),
                    point(bounds.left() + end.x, top(start) + line_height),
                ));
            } else {
                selection.push(Bounds::from_corners(
                    point(bounds.left() + start.x, top(start)),
                    point(bounds.right(), top(start) + line_height),
                ));
                if end.y > start.y + line_height {
                    selection.push(Bounds::from_corners(
                        point(bounds.left(), top(start) + line_height),
                        point(bounds.right(), top(end)),
                    ));
                }
                selection.push(Bounds::from_corners(
                    point(bounds.left(), top(end)),
                    point(bounds.left() + end.x, top(end) + line_height),
                ));
            }
        }

        let rows_changed = total_rows != area.rows;
        self.area.update(cx, |area, _| {
            area.rows = total_rows;
            area.scroll = scroll;
            area.layout = Some(layout);
        });
        if rows_changed {
            window.refresh();
        }
        Prepaint {
            shift,
            selection,
            cursor,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.area.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.area.clone()),
            cx,
        );
        let line_height = px(LINE_HEIGHT);
        // The layout lives in the field (to map clicks and arrows); take it
        // out while painting, since painting needs the whole app.
        let layout = self.area.update(cx, |area, _| area.layout.take());
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for rect in prepaint.selection.drain(..) {
                window.paint_quad(fill(rect, hsla(theme::ACCENT).opacity(0.3)));
            }
            if let Some(layout) = &layout {
                for (_, row, line) in &layout.lines {
                    let origin = point(
                        bounds.left(),
                        bounds.top() + line_height * *row as f32 - prepaint.shift,
                    );
                    let _ = line.paint(
                        origin,
                        line_height,
                        TextAlign::Left,
                        Some(bounds),
                        window,
                        cx,
                    );
                }
            }
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(fill(cursor, color(theme::ACCENT)));
            }
        });
        self.area.update(cx, |area, _| area.layout = layout);
    }
}

fn clamp_range(text: &str, range: Range<usize>) -> Range<usize> {
    let fix = |mut offset: usize| {
        offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    };
    let (start, end) = (fix(range.start), fix(range.end));
    start.min(end)..start.max(end)
}

fn previous_char(text: &str, offset: usize) -> usize {
    text[..offset]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}

fn next_char(text: &str, offset: usize) -> usize {
    text[offset..]
        .chars()
        .next()
        .map_or(text.len(), |ch| offset + ch.len_utf8())
}

fn previous_word(text: &str, offset: usize) -> usize {
    let before = &text[..offset];
    let trimmed = before.trim_end_matches(|c: char| !c.is_alphanumeric());
    trimmed
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_alphanumeric())
        .map_or(0, |(index, c)| index + c.len_utf8())
}

fn next_word(text: &str, offset: usize) -> usize {
    let after = &text[offset..];
    let skip = after.len()
        - after
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .len();
    let rest = &after[skip..];
    let word = rest
        .char_indices()
        .find(|(_, c)| !c.is_alphanumeric())
        .map_or(rest.len(), |(index, _)| index);
    offset + skip + word
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_steps_respect_utf8() {
        let text = "año ñ";
        assert_eq!(next_char(text, 1), 3);
        assert_eq!(previous_char(text, 3), 1);
        assert_eq!(previous_char(text, 0), 0);
        assert_eq!(next_char(text, text.len()), text.len());
    }

    #[test]
    fn word_steps_skip_punctuation() {
        let text = "hola, qué tal";
        assert_eq!(next_word(text, 0), 4);
        assert_eq!(next_word(text, 4), 10);
        assert_eq!(previous_word(text, text.len()), 11);
        assert_eq!(previous_word(text, 10), 6);
        assert_eq!(previous_word(text, 3), 0);
    }

    #[test]
    fn ranges_are_clamped_to_char_boundaries() {
        let text = "ñu";
        assert_eq!(clamp_range(text, 1..9), 0..3);
        #[allow(clippy::reversed_empty_ranges)]
        let reversed = 3..0;
        assert_eq!(clamp_range(text, reversed), 0..3);
    }
}
