use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use titan_agent::app::MainWindow;

fn main() {
    // TITAN_AGENT_SELECT supports "new", "project:<name>", and "session:<id>" for demo startup selection.
    application().run(|cx: &mut App| {
        if let Err(error) = titan_agent::fonts::load(cx) {
            eprintln!("could not load embedded fonts: {error}");
        }
        let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(MainWindow::new),
        );
        cx.activate(true);
    });
}
