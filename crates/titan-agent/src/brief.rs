//! The brief field of the "new session" box: a small multi-line text editor.
//!
//! GPUI ships no text field (ADR-0003), so this follows its `input.rs`
//! example: the platform input handler delivers typed text and IME
//! composition, key presses handle editing and movement, and a custom element
//! shapes the text with wrapping. The text is only ever what the user typed
//! (ADR-0002): nothing here completes or rewrites it.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, KeyDownEvent,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ScrollEvent, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, WrappedLine,
    fill, point, prelude::*, px, relative, size,
};

use crate::theme::{self, color, hsla};

/// Visible lines of the field: it grows with the text up to the maximum and
/// then scrolls.
const MIN_LINES: usize = 4;
const MAX_LINES: usize = 12;
pub const FONT_SIZE: f32 = 13.;
pub const LINE_HEIGHT: f32 = 19.;

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

/// Events for the view that owns the field.
pub enum BriefEvent {
    Changed,
    /// ctrl+enter: the user asks to launch.
    Submit,
}

/// One hard line of the last layout: its shaped text, where it starts in
/// the text and its top inside the content.
struct LaidLine {
    line: WrappedLine,
    start: usize,
    top: Pixels,
}

pub struct BriefInput {
    focus: FocusHandle,
    pub content: BriefText,
    placeholder: SharedString,
    lines: Vec<LaidLine>,
    /// Content origin on screen (bounds origin minus the scroll).
    origin: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
    visual_lines: usize,
    scroll: Pixels,
    selecting: bool,
    /// Wanted x while moving up and down.
    goal_x: Option<Pixels>,
    /// Scroll to the cursor on the next paint (after an edit or a move,
    /// not after the wheel).
    reveal: bool,
}

impl EventEmitter<BriefEvent> for BriefInput {}

impl BriefInput {
    pub fn new(placeholder: &str, cx: &mut Context<Self>) -> BriefInput {
        BriefInput {
            focus: cx.focus_handle(),
            content: BriefText::default(),
            placeholder: placeholder.to_string().into(),
            lines: Vec::new(),
            origin: Point::default(),
            bounds: None,
            visual_lines: MIN_LINES,
            scroll: px(0.),
            selecting: false,
            goal_x: None,
            reveal: true,
        }
    }

    pub fn text(&self) -> &str {
        &self.content.text
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content = BriefText::default();
        self.scroll = px(0.);
        cx.emit(BriefEvent::Changed);
        cx.notify();
    }

    fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal = true;
        window.notify_text_input_changed(&self.focus);
        cx.emit(BriefEvent::Changed);
        cx.notify();
    }

    fn moved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal = true;
        window.notify_text_selection_changed(&self.focus);
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let m = keystroke.modifiers;
        let ctrl = m.control || m.platform;
        let shift = m.shift;
        let text = &mut self.content;
        let cursor = text.cursor();
        let mut keep_goal = false;
        match keystroke.key.as_str() {
            "enter" if ctrl => {
                cx.emit(BriefEvent::Submit);
            }
            "enter" => {
                text.replace(None, "\n");
                self.goal_x = None;
                self.changed(window, cx);
            }
            "backspace" | "delete" => {
                let forward = keystroke.key == "delete";
                if ctrl && text.selection.is_empty() {
                    let other = if forward {
                        text.next_word(cursor)
                    } else {
                        text.previous_word(cursor)
                    };
                    text.selection = cursor.min(other)..cursor.max(other);
                }
                text.delete(forward);
                self.changed(window, cx);
            }
            "left" => {
                let to = if ctrl {
                    text.previous_word(cursor)
                } else if !shift && !text.selection.is_empty() {
                    text.selection.start
                } else {
                    text.previous_boundary(cursor)
                };
                text.go(to, shift);
                self.moved(window, cx);
            }
            "right" => {
                let to = if ctrl {
                    text.next_word(cursor)
                } else if !shift && !text.selection.is_empty() {
                    text.selection.end
                } else {
                    text.next_boundary(cursor)
                };
                text.go(to, shift);
                self.moved(window, cx);
            }
            "up" | "down" => {
                let down = keystroke.key == "down";
                if let Some(to) = self.vertical(down) {
                    self.content.go(to, shift);
                }
                keep_goal = true;
                self.moved(window, cx);
            }
            "home" => {
                let to = if ctrl { 0 } else { text.line_start(cursor) };
                text.go(to, shift);
                self.moved(window, cx);
            }
            "end" => {
                let to = if ctrl {
                    text.text.len()
                } else {
                    text.line_end(cursor)
                };
                text.go(to, shift);
                self.moved(window, cx);
            }
            "a" if ctrl => {
                text.select_all();
                self.moved(window, cx);
            }
            "c" | "x" if ctrl => {
                if !text.selection.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        text.selected_text().to_string(),
                    ));
                    if keystroke.key == "x" {
                        text.replace(None, "");
                        self.changed(window, cx);
                    }
                }
            }
            "v" if ctrl => {
                if let Some(paste) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    text.replace(None, &paste.replace("\r\n", "\n"));
                    self.changed(window, cx);
                }
            }
            "escape" => {
                if text.selection.is_empty() {
                    return;
                }
                text.move_to(cursor);
                self.moved(window, cx);
            }
            // Plain text goes through the platform input handler.
            _ => return,
        }
        if !keep_goal {
            self.goal_x = None;
        }
        cx.stop_propagation();
    }

    /// Offset one visual line above or below the cursor, keeping its x.
    fn vertical(&mut self, down: bool) -> Option<usize> {
        let line_height = px(LINE_HEIGHT);
        let position = self.position_for_offset(self.content.cursor())?;
        let x = *self.goal_x.get_or_insert(position.x);
        let y = if down {
            position.y + line_height * 1.5
        } else {
            position.y - line_height * 0.5
        };
        if y < px(0.) {
            return Some(0);
        }
        let total = line_height * self.visual_count() as f32;
        if y >= total {
            return Some(self.content.text.len());
        }
        Some(self.offset_for_content_point(point(x, y)))
    }

    fn visual_count(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.line.wrap_boundaries().len() + 1)
            .sum()
    }

    /// Top-left of the char at `offset`, in content coordinates.
    fn position_for_offset(&self, offset: usize) -> Option<Point<Pixels>> {
        let line_height = px(LINE_HEIGHT);
        let laid = self.lines.iter().rev().find(|l| l.start <= offset)?;
        let local = laid
            .line
            .position_for_index(offset - laid.start, line_height)?;
        Some(point(local.x, laid.top + local.y))
    }

    fn offset_for_content_point(&self, p: Point<Pixels>) -> usize {
        let line_height = px(LINE_HEIGHT);
        let Some(laid) = self
            .lines
            .iter()
            .rev()
            .find(|l| l.top <= p.y)
            .or(self.lines.first())
        else {
            return 0;
        };
        let local = point(p.x.max(px(0.)), (p.y - laid.top).max(px(0.)));
        let index = match laid.line.closest_index_for_position(local, line_height) {
            Ok(i) | Err(i) => i,
        };
        (laid.start + index).min(self.content.text.len())
    }

    fn offset_for_mouse(&self, position: Point<Pixels>) -> usize {
        self.offset_for_content_point(position - self.origin)
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        self.selecting = true;
        let offset = self.offset_for_mouse(event.position);
        if event.click_count >= 2 && !event.modifiers.shift {
            let start = self
                .content
                .previous_word(self.content.next_boundary(offset));
            let end = self.content.next_word(start);
            self.content.selection = start..end;
            self.content.reversed = false;
        } else {
            self.content.go(offset, event.modifiers.shift);
        }
        self.goal_x = None;
        self.moved(window, cx);
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            let offset = self.offset_for_mouse(event.position);
            self.content.select_to(offset);
            self.moved(window, cx);
        }
    }

    fn scroll_wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(LINE_HEIGHT)).y;
        self.scroll = (self.scroll - delta).clamp(px(0.), self.max_scroll());
        cx.stop_propagation();
        cx.notify();
    }

    fn max_scroll(&self) -> Pixels {
        let shown = self.visual_lines.clamp(MIN_LINES, MAX_LINES);
        px(LINE_HEIGHT) * self.visual_lines.saturating_sub(shown) as f32
    }
}

impl EntityInputHandler for BriefInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.content.range_from_utf16(&range_utf16);
        actual_range.replace(self.content.range_to_utf16(&range));
        Some(self.content.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.content.range_to_utf16(&self.content.selection),
            reversed: self.content.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.content
            .marked
            .as_ref()
            .map(|range| self.content.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.content.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16.map(|r| self.content.range_from_utf16(&r));
        self.content.replace(range, new_text);
        self.goal_x = None;
        self.changed(window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|r| self.content.range_from_utf16(&r))
            .or(self.content.marked.clone())
            .unwrap_or(self.content.selection.clone());
        self.content.text.replace_range(range.clone(), new_text);
        self.content.marked =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        let start16 = self.content.offset_to_utf16(range.start);
        let len16 = new_text.encode_utf16().count();
        self.content.selection = match new_selected_range_utf16 {
            Some(sel) => self
                .content
                .range_from_utf16(&(start16 + sel.start.min(len16)..start16 + sel.end.min(len16))),
            None => range.start + new_text.len()..range.start + new_text.len(),
        };
        self.content.reversed = false;
        self.changed(window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.content.range_from_utf16(&range_utf16);
        let start = self.position_for_offset(range.start)?;
        Some(Bounds::new(
            self.origin + start,
            size(px(2.), px(LINE_HEIGHT)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.offset_for_mouse(p);
        Some(self.content.offset_to_utf16(offset))
    }
}

impl Focusable for BriefInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for BriefInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = self.visual_lines.clamp(MIN_LINES, MAX_LINES);
        gpui::div()
            .id("brief")
            .key_context("Brief")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_scroll(cx.listener(Self::scroll_wheel))
            .p(px(12.))
            .w_full()
            .h(px(LINE_HEIGHT * shown as f32 + 24.))
            .text_size(px(FONT_SIZE))
            .line_height(px(LINE_HEIGHT))
            .child(BriefElement { input: cx.entity() })
    }
}

struct BriefElement {
    input: gpui::Entity<BriefInput>,
}

struct Prepainted {
    selection: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for BriefElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for BriefElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepainted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepainted {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let empty = content.text.is_empty();
        let style = window.text_style();
        let font_size = px(FONT_SIZE);
        let line_height = px(LINE_HEIGHT);
        let (text, text_color): (SharedString, _) = if empty {
            (input.placeholder.clone(), hsla(theme::MUTE))
        } else {
            (content.text.clone().into(), hsla(theme::BODY))
        };
        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = match &content.marked {
            Some(marked) if !empty => vec![
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.end - marked.start,
                    underline: Some(UnderlineStyle {
                        color: Some(text_color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: text.len() - marked.end,
                    ..run
                },
            ],
            _ => vec![run],
        };
        let shaped = window
            .text_system()
            .shape_text(text, font_size, &runs, Some(bounds.size.width), None)
            .unwrap_or_default();

        let mut laid = Vec::new();
        let mut start = 0;
        let mut top = px(0.);
        for line in shaped {
            let rows = line.wrap_boundaries().len() + 1;
            let len = line.len();
            laid.push(LaidLine {
                line,
                start: if empty { 0 } else { start },
                top,
            });
            start += len + 1;
            top += line_height * rows as f32;
        }
        let visual_lines = laid
            .iter()
            .map(|l| l.line.wrap_boundaries().len() + 1)
            .sum::<usize>();

        let focused = input.focus.is_focused(window);
        let mut scroll = input.scroll;
        let cursor_offset = if empty { 0 } else { content.cursor() };
        // Keep the cursor visible.
        let cursor_at = {
            let probe = BriefProbe { lines: &laid };
            probe.position(cursor_offset)
        };
        if let Some(at) = cursor_at.filter(|_| input.reveal) {
            if at.y < scroll {
                scroll = at.y;
            } else if at.y + line_height > scroll + bounds.size.height {
                scroll = at.y + line_height - bounds.size.height;
            }
        }
        let max_scroll = (top - bounds.size.height).max(px(0.));
        scroll = scroll.clamp(px(0.), max_scroll);
        let origin = point(bounds.origin.x, bounds.origin.y - scroll);

        let probe = BriefProbe { lines: &laid };
        let mut selection = Vec::new();
        if !empty && !content.selection.is_empty() {
            let (Some(a), Some(b)) = (
                probe.position(content.selection.start),
                probe.position(content.selection.end),
            ) else {
                return Prepainted {
                    selection,
                    cursor: None,
                };
            };
            let highlight = color(theme::ACCENT).alpha(0.35);
            let mut y = a.y;
            while y <= b.y {
                let left = if y == a.y { a.x } else { px(0.) };
                let right = if y == b.y { b.x } else { bounds.size.width };
                if right > left {
                    selection.push(fill(
                        Bounds::from_corners(
                            origin + point(left, y),
                            origin + point(right, y + line_height),
                        ),
                        highlight,
                    ));
                }
                y += line_height;
            }
        }
        let cursor = (focused && content.marked.is_none())
            .then_some(cursor_at)
            .flatten()
            .map(|at| {
                fill(
                    Bounds::new(origin + at, size(px(2.), line_height)),
                    color(theme::ACCENT),
                )
            });

        self.input.update(cx, |input, cx| {
            input.lines = laid;
            input.origin = origin;
            input.bounds = Some(bounds);
            input.scroll = scroll;
            input.reveal = false;
            if input.visual_lines != visual_lines {
                input.visual_lines = visual_lines;
                cx.notify();
            }
        });
        Prepainted { selection, cursor }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepainted: &mut Prepainted,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus.clone();
        let origin = self.input.read(cx).origin;
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for quad in prepainted.selection.drain(..) {
                window.paint_quad(quad);
            }
            self.input.update(cx, |input, cx| {
                // The placeholder starts after the caret instead of under it.
                let indent = if input.content.text.is_empty() {
                    px(4.)
                } else {
                    px(0.)
                };
                for laid in &input.lines {
                    let _ = laid.line.paint(
                        origin + point(indent, laid.top),
                        px(LINE_HEIGHT),
                        gpui::TextAlign::Left,
                        Some(bounds),
                        window,
                        cx,
                    );
                }
            });
            if let Some(cursor) = prepainted.cursor.take() {
                window.paint_quad(cursor);
            }
        });
    }
}

/// Position lookups on a layout that is not stored yet.
struct BriefProbe<'a> {
    lines: &'a [LaidLine],
}

impl BriefProbe<'_> {
    fn position(&self, offset: usize) -> Option<Point<Pixels>> {
        let laid = self.lines.iter().rev().find(|l| l.start <= offset)?;
        let local = laid
            .line
            .position_for_index(offset - laid.start, px(LINE_HEIGHT))?;
        Some(point(local.x, laid.top + local.y))
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
