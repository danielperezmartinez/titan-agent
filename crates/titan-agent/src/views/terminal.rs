use crate::{app::MainWindow, terminal_model::CellSpan, theme};
use gpui::{
    App, Bounds, FontStyle, FontWeight, IntoElement, SharedString, StrikethroughStyle, TextRun,
    UnderlineStyle, div, fill, point, prelude::*, px, rgb, size,
};
use gpui::{WeakEntity, Window};

pub(crate) fn terminal_canvas(
    lines: Vec<Vec<CellSpan>>,
    cursor: Option<(u16, u16)>,
    owner: WeakEntity<MainWindow>,
) -> impl IntoElement {
    div()
        .size_full()
        .bg(rgb(theme::TERMINAL_BG))
        .child(gpui::canvas(
            move |bounds, window: &mut Window, cx: &mut App| {
                let text_style = window.text_style();
                let font_size = text_style.font_size.to_pixels(window.rem_size());
                let measure_run = TextRun {
                    len: 1,
                    font: text_style.font(),
                    color: text_style.color,
                    ..Default::default()
                };
                let cell_width = window
                    .text_system()
                    .shape_line(SharedString::from("M"), font_size, &[measure_run], None)
                    .width();
                let line_height = window.line_height();
                let cols = (f32::from(bounds.size.width) / f32::from(cell_width)).floor() as u16;
                let rows = (f32::from(bounds.size.height) / f32::from(line_height)).floor() as u16;
                let owner = owner.clone();
                cx.defer(move |cx| {
                    let _ = owner.update(cx, |view, cx| view.resize_terminal(cols, rows, cx));
                });
                (
                    cell_width,
                    line_height,
                    font_size,
                    text_style,
                    bounds,
                    cols,
                    rows,
                )
            },
            move |bounds,
                  (cell_width, line_height, font_size, text_style, _, cols, rows),
                  window,
                  cx| {
                window.paint_quad(fill(bounds, rgb(theme::TERMINAL_BG)));
                for (row, spans) in lines.iter().enumerate().take(usize::from(rows)) {
                    for item in spans {
                        if item.column >= cols {
                            continue;
                        }
                        let x = bounds.left() + cell_width * f32::from(item.column);
                        let y = bounds.top() + line_height * row as f32;
                        let width = cell_width
                            * f32::from(item.cells.min(cols.saturating_sub(item.column)));
                        window.paint_quad(fill(
                            Bounds::new(point(x, y), size(width, line_height)),
                            rgb(item.span.bg),
                        ));
                        let mut font = text_style.font();
                        if item.span.bold {
                            font.weight = FontWeight::BOLD;
                        }
                        if item.span.italic {
                            font.style = FontStyle::Italic;
                        }
                        let run = TextRun {
                            len: item.span.text.len(),
                            font,
                            color: rgb(item.span.fg).into(),
                            underline: item.span.underline.then_some(UnderlineStyle {
                                thickness: px(1.),
                                color: Some(rgb(item.span.fg).into()),
                                wavy: false,
                            }),
                            strikethrough: item.span.strikeout.then_some(StrikethroughStyle {
                                thickness: px(1.),
                                color: Some(rgb(item.span.fg).into()),
                            }),
                            ..Default::default()
                        };
                        let shaped = window.text_system().shape_line(
                            SharedString::from(item.span.text.clone()),
                            font_size,
                            &[run],
                            None,
                        );
                        let _ = shaped.paint(
                            point(x, y),
                            line_height,
                            gpui::TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                    }
                }
                if let Some((row, col)) = cursor.filter(|(r, c)| *r < rows && *c < cols) {
                    let x = bounds.left() + cell_width * f32::from(col);
                    let y = bounds.top() + line_height * f32::from(row);
                    window.paint_quad(fill(
                        Bounds::new(point(x, y), size(cell_width, line_height)),
                        rgb(theme::TERMINAL_CURSOR),
                    ));
                }
            },
        ))
}
