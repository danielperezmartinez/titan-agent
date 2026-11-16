//! titan-agent: GPUI desktop interface, a client of titan-agentd (ADR-0004).

mod app;
mod brief;
mod client;
mod config;
mod keys;
mod terminal;
mod theme;

use std::borrow::Cow;

use gpui::{App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_platform::application;

use crate::app::MainView;
use crate::client::DaemonClient;

/// JetBrains Mono (SIL Open Font License 1.1, see assets/fonts/OFL.txt).
fn load_fonts(cx: &App) {
    let fonts = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf").as_slice()),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("titan-agent: no se pudo cargar JetBrains Mono: {error}");
    }
}

fn main() {
    if std::env::args().any(|arg| arg == "--version" || arg == "-V") {
        println!("titan-agent {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    application().run(|cx: &mut App| {
        load_fonts(cx);
        let (client, incoming) = DaemonClient::start();
        let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("titan-agent".into()),
                    ..Default::default()
                }),
                app_id: Some("titan-agent".into()),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| MainView::new(client, incoming, cx)),
        )
        .expect("failed to open the main window");
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
