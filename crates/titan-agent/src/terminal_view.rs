//! GPUI view for a terminal grid.

use crate::{
    terminal_model::{TerminalModel, palette},
    theme,
    view_model::grid_size,
};
use gpui::{
    App, Bounds, Context, EventEmitter, FocusHandle, Focusable, Font, FontStyle, FontWeight, Hsla,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Render, ScrollDelta, SharedString,
    StrikethroughStyle, Styled, TextAlign, TextRun, UnderlineStyle, Window, div, fill, point, px,
    rgb, size,
};
use titan_agent_core::keys::{KeyMods, encode_key, encode_paste};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalViewEvent {
    Input(Vec<u8>),
    Resized { cols: u16, rows: u16 },
}

pub struct TerminalView {
    model: TerminalModel,
    focus: FocusHandle,
    cell_width: f32,
    scale_factor: f32,
    grid: Option<(u16, u16)>,
    scroll_remainder: f32,
}

impl EventEmitter<TerminalViewEvent> for TerminalView {}
impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TerminalView {
    pub const FONT_SIZE: f32 = 13.0;
    pub const LINE_HEIGHT: f32 = 19.0;
    pub const PADDING: f32 = 16.0;

    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            model: TerminalModel::new(80, 24),
            focus: cx.focus_handle(),
            cell_width: 8.0,
            scale_factor: 0.0,
            grid: None,
            scroll_remainder: 0.0,
        }
    }

    pub fn model(&self) -> &TerminalModel {
        &self.model
    }

    pub fn load_snapshot(&mut self, cols: u16, rows: u16, data: &[u8], cx: &mut Context<Self>) {
        self.model.load_snapshot(cols, rows, data);
        cx.notify();
    }

    pub fn feed(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.model.feed(bytes);
        cx.notify();
    }

    pub fn grid(&self) -> Option<(u16, u16)> {
        self.grid
    }
}

fn color(value: u32) -> Hsla {
    rgb(value).into()
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font = Font {
            family: SharedString::from(theme::FONT_FAMILY),
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
            ..Font::default()
        };
        let scale = window.scale_factor();
        if self.scale_factor != scale
            && let Ok(advance) = window.text_system().advance(
                window.text_system().resolve_font(&font),
                px(Self::FONT_SIZE),
                'm',
            )
        {
            self.cell_width = advance.width.as_f32();
            self.scale_factor = scale;
        }
        let frame = self.model.frame();
        let cw = self.cell_width;
        let lh = Self::LINE_HEIGHT;
        let focus = self.focus.clone();
        let owner = cx.entity();
        let resize_owner = owner.clone();
        div()
            .size_full()
            .bg(rgb(palette::DEFAULT_BG))
            .track_focus(&focus)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            })
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                let key = &event.keystroke;
                let paste = (key.key == "v" && key.modifiers.control && key.modifiers.shift)
                    || (key.key == "insert" && key.modifiers.shift);
                if paste {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        let bytes = encode_paste(&text, this.model.modes().bracketed_paste);
                        this.model.scroll_to_bottom();
                        cx.emit(TerminalViewEvent::Input(bytes));
                    }
                    cx.stop_propagation();
                } else if let Some(bytes) = encode_key(
                    &key.key,
                    key.key_char.as_deref(),
                    KeyMods {
                        ctrl: key.modifiers.control,
                        alt: key.modifiers.alt,
                        shift: key.modifiers.shift,
                        platform: key.modifiers.platform,
                    },
                    this.model.modes().app_cursor,
                ) {
                    this.model.scroll_to_bottom();
                    cx.emit(TerminalViewEvent::Input(bytes));
                    cx.stop_propagation();
                }
            }))
            .on_scroll(cx.listener(move |this, event: &gpui::ScrollEvent, _, cx| {
                let delta = match event.delta {
                    ScrollDelta::Pixels(point) => -point.y.as_f32() / Self::LINE_HEIGHT,
                    ScrollDelta::Lines(point) => -point.y,
                };
                this.scroll_remainder += delta;
                let lines = this.scroll_remainder.trunc() as i32;
                this.scroll_remainder -= lines as f32;
                if lines == 0 {
                    return;
                }
                let modes = this.model.modes();
                if modes.alt_screen || modes.mouse_reporting {
                    for _ in 0..lines.unsigned_abs() {
                        if let Some(bytes) = encode_key(
                            if lines > 0 { "up" } else { "down" },
                            None,
                            KeyMods::default(),
                            modes.app_cursor,
                        ) {
                            cx.emit(TerminalViewEvent::Input(bytes));
                        }
                    }
                } else {
                    this.model.scroll(lines);
                    cx.notify();
                }
            }))
            .child(
                gpui::canvas(
                    move |bounds, _window, app| {
                        if bounds.size.width.as_f32() <= 0.0 || bounds.size.height.as_f32() <= 0.0 {
                            return;
                        }
                        let next = grid_size(
                            bounds.size.width.as_f32(),
                            bounds.size.height.as_f32(),
                            Self::PADDING,
                            cw,
                            lh,
                        );
                        if resize_owner.read(app).grid != Some(next) {
                            let owner = resize_owner.clone();
                            app.defer(move |app| {
                                owner.update(app, |view, cx| {
                                    if view.grid != Some(next) {
                                        view.grid = Some(next);
                                        view.model.resize(next.0, next.1);
                                        cx.emit(TerminalViewEvent::Resized {
                                            cols: next.0,
                                            rows: next.1,
                                        });
                                        cx.notify();
                                    }
                                });
                            });
                        }
                    },
                    move |bounds, _, window, cx| {
                        window.paint_quad(fill(bounds, rgb(palette::DEFAULT_BG)));
                        for (line_index, line) in frame.lines.iter().enumerate() {
                            for run in &line.runs {
                                let x = bounds.origin.x.as_f32()
                                    + Self::PADDING
                                    + f32::from(run.col) * cw;
                                let y = bounds.origin.y.as_f32()
                                    + Self::PADDING
                                    + line_index as f32 * lh;
                                if let Some(bg) = run.bg {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(px(x), px(y)),
                                            size(px(f32::from(run.cells) * cw), px(lh)),
                                        ),
                                        rgb(bg),
                                    ));
                                }
                                let run_font = Font {
                                    weight: if run.bold {
                                        FontWeight::BOLD
                                    } else {
                                        FontWeight::NORMAL
                                    },
                                    style: if run.italic {
                                        FontStyle::Italic
                                    } else {
                                        FontStyle::Normal
                                    },
                                    ..Font {
                                        family: SharedString::from(theme::FONT_FAMILY),
                                        ..Font::default()
                                    }
                                };
                                let text_run = TextRun {
                                    len: run.text.len(),
                                    font: run_font,
                                    color: color(run.fg),
                                    background_color: None,
                                    underline: run.underline.then_some(UnderlineStyle {
                                        thickness: px(1.),
                                        ..Default::default()
                                    }),
                                    strikethrough: run.strikeout.then_some(StrikethroughStyle {
                                        thickness: px(1.),
                                        ..Default::default()
                                    }),
                                };
                                let wide = run.cells as usize != run.text.chars().count();
                                let _ = window
                                    .text_system()
                                    .shape_line(
                                        run.text.clone().into(),
                                        px(Self::FONT_SIZE),
                                        &[text_run],
                                        (!wide).then_some(px(cw)),
                                    )
                                    .paint(
                                        point(px(x), px(y)),
                                        px(lh),
                                        TextAlign::Left,
                                        None,
                                        window,
                                        cx,
                                    );
                            }
                        }
                        if let Some(cursor) = frame.cursor {
                            let x = bounds.origin.x.as_f32()
                                + Self::PADDING
                                + f32::from(cursor.col) * cw;
                            let y = bounds.origin.y.as_f32()
                                + Self::PADDING
                                + f32::from(cursor.line) * lh;
                            let cursor_bounds =
                                Bounds::new(point(px(x), px(y)), size(px(cw), px(lh)));
                            if owner.read(cx).focus.is_focused(window) {
                                window.paint_quad(fill(cursor_bounds, rgb(palette::CURSOR)));
                                if let Some(ch) =
                                    frame.lines.get(cursor.line as usize).and_then(|line| {
                                        line.runs.iter().find_map(|run| {
                                            let offset = cursor.col.checked_sub(run.col)? as usize;
                                            char_at_cell(&run.text, offset)
                                        })
                                    })
                                {
                                    let text_run = TextRun {
                                        len: ch.len_utf8(),
                                        font: Font {
                                            family: SharedString::from(theme::FONT_FAMILY),
                                            ..Font::default()
                                        },
                                        color: color(palette::DEFAULT_FG),
                                        background_color: None,
                                        underline: None,
                                        strikethrough: None,
                                    };
                                    let _ = window
                                        .text_system()
                                        .shape_line(
                                            ch.to_string().into(),
                                            px(Self::FONT_SIZE),
                                            &[text_run],
                                            None,
                                        )
                                        .paint(
                                            point(px(x), px(y)),
                                            px(lh),
                                            TextAlign::Left,
                                            None,
                                            window,
                                            cx,
                                        );
                                }
                            } else {
                                let edge = rgb(palette::CURSOR);
                                window.paint_quad(fill(
                                    Bounds::new(point(px(x), px(y)), size(px(cw), px(1.))),
                                    edge,
                                ));
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(px(x), px(y + lh - 1.)),
                                        size(px(cw), px(1.)),
                                    ),
                                    edge,
                                ));
                                window.paint_quad(fill(
                                    Bounds::new(point(px(x), px(y)), size(px(1.), px(lh))),
                                    edge,
                                ));
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(px(x + cw - 1.), px(y)),
                                        size(px(1.), px(lh)),
                                    ),
                                    edge,
                                ));
                            }
                        }
                    },
                )
                .size_full(),
            )
    }
}

fn char_at_cell(text: &str, target: usize) -> Option<char> {
    let mut cell = 0;
    for ch in text.chars() {
        if cell == target {
            return Some(ch);
        }
        cell += if ch as u32 >= 0x1100 { 2 } else { 1 };
    }
    None
}
