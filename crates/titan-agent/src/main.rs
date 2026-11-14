//! titan-agent: GPUI desktop interface. For now it only opens an empty window
//! with the canvas colour, to prove that the pinned GPUI builds and runs.

use gpui::{
    App, Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;

/// `canvas` token inherited from titan-ssh.
const CANVAS: u32 = 0x201d1d;
/// `ink` token inherited from titan-ssh.
const INK: u32 = 0xfdfcfc;

struct MainWindow;

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .justify_center()
            .items_center()
            .bg(rgb(CANVAS))
            .text_color(rgb(INK))
            .child("titan-agent")
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| MainWindow),
        )
        .expect("failed to open the main window");
        cx.activate(true);
    });
}
