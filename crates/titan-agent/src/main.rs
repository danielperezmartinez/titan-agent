//! titan-agent: GPUI desktop interface, a client of titan-agentd (ADR-0004).

mod app;
mod client;
mod config;
mod terminal;
mod theme;

use std::borrow::Cow;
use std::time::Duration;

use gpui::{App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_platform::application;

use crate::app::MainView;
use crate::client::DaemonClient;
use titan_agent::demo::{DemoStep, parse_keys, parse_size};

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

/// A demo variable (scripts/demo/): parsed before opening the window, so a
/// wrong value ends the program instead of capturing the wrong thing.
fn demo_var<T>(name: &str, parse: impl Fn(&str) -> Result<T, String>) -> Option<T> {
    let value = std::env::var(name).ok()?;
    match parse(&value) {
        Ok(parsed) => Some(parsed),
        Err(error) => {
            eprintln!("titan-agent: {name}: {error}");
            std::process::exit(2);
        }
    }
}

/// Plays a demo key script on the window, as if the keys were pressed:
/// 1.5 s after it opens, 20 ms between keys.
fn play_keys(window: gpui::AnyWindowHandle, steps: Vec<DemoStep>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let timer =
            |cx: &gpui::AsyncApp, ms| cx.background_executor().timer(Duration::from_millis(ms));
        timer(cx, 1500).await;
        for step in steps {
            match step {
                DemoStep::Pause(wait) => {
                    cx.background_executor().timer(wait).await;
                }
                DemoStep::Key(keystroke) => {
                    // Through the window, not the view: the key handlers
                    // update the view themselves.
                    let sent = cx.update_window(window, |_, window, cx| {
                        window.dispatch_keystroke(keystroke, cx);
                    });
                    if sent.is_err() {
                        return;
                    }
                    timer(cx, 20).await;
                }
            }
        }
        // The demo kit waits for this line before capturing.
        eprintln!("titan-agent: guion de teclas terminado");
    })
    .detach();
}

fn main() {
    if std::env::args().any(|arg| arg == "--version" || arg == "-V") {
        println!("titan-agent {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let (width, height) = demo_var("TITAN_AGENT_DEMO_SIZE", parse_size).unwrap_or((1440, 900));
    let keys = demo_var("TITAN_AGENT_DEMO_KEYS", parse_keys);
    application().run(move |cx: &mut App| {
        load_fonts(cx);
        let (client, incoming) = DaemonClient::start();
        let bounds = Bounds::centered(None, size(px(width as f32), px(height as f32)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("titan-agent".into()),
                        ..Default::default()
                    }),
                    // The id of the menu entry, so Plasma ties the window to it.
                    app_id: Some(titan_agent_core::update::APP_ID.into()),
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| MainView::new(client, incoming, cx)),
            )
            .expect("failed to open the main window");
        if let Some(steps) = keys.clone() {
            play_keys(window.into(), steps, cx);
        }
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
