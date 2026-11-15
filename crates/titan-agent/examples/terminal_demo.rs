use gpui::{App, Bounds, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size};
use gpui_platform::application;
use titan_agent::{
    fonts,
    terminal_view::{TerminalView, TerminalViewEvent},
    theme,
};

struct Demo {
    terminal: gpui::Entity<TerminalView>,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(theme::TERMINAL_BG))
            .child(self.terminal.clone())
    }
}

fn main() {
    application().run(|cx: &mut App| {
        fonts::load(cx).expect("failed to load terminal fonts");
        let bounds = Bounds::centered(None, size(px(900.), px(600.)), cx);
        cx.open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() }, |_, cx| {
            let terminal = cx.new(TerminalView::new);
            terminal.update(cx, |view, cx| view.load_snapshot(80, 24, "\x1b[1;36mTitan Agent terminal demo\x1b[0m\r\n\x1b[33mColors\x1b[0m  \x1b[1mBold\x1b[0m  \x1b[4mUnderline\x1b[0m  wide: 界\r\nA deliberately long line: 0123456789 abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ".as_bytes(), cx));
            let echo_terminal = terminal.clone();
            cx.new(|cx| {
                cx.subscribe(&terminal, move |_, _, event: &TerminalViewEvent, cx| {
                    if let TerminalViewEvent::Input(bytes) = event {
                        let echo = String::from_utf8_lossy(bytes).replace('\r', "\r\n");
                        echo_terminal.update(cx, |view, cx| view.feed(echo.as_bytes(), cx));
                    }
                }).detach();
                Demo { terminal }
            })
        }).expect("failed to open the demo window");
        cx.activate(true);
    });
}
