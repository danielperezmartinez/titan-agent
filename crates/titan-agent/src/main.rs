use gpui::{
    App, Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;
use std::{sync::mpsc, time::Duration};
use titan_agent::{
    client::{ClientError, Connection, connect_or_spawn, daemon_program, default_socket_path},
    theme,
};
use titan_agent_core::{
    protocol::{ClientRequest, Session},
    session::StateCounts,
    tree::build_tree,
};
struct MainWindow {
    status: String,
    sessions: Vec<Session>,
    connection: Option<Connection>,
    selected: Option<String>,
    filter: String,
}
impl MainWindow {
    fn start(&mut self, cx: &mut Context<Self>) {
        self.status = "conectando con titan-agentd…".into();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let res = connect_or_spawn(
                &default_socket_path(),
                &daemon_program(),
                Duration::from_secs(5),
            )
            .and_then(|c| {
                let v = c.request(ClientRequest::SessionsSubscribe)?;
                let sessions =
                    serde_json::from_value::<titan_agent_core::protocol::SessionsResult>(v)
                        .map_err(|e| ClientError::Protocol(e.to_string()))?
                        .sessions;
                Ok((c, sessions))
            });
            let _ = tx.send(res);
        });
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { rx.recv().ok() })
                .await;
            let _ = this.update(cx, |view, cx| {
                match result {
                    Some(Ok((connection, sessions))) => {
                        view.connection = Some(connection);
                        view.sessions = sessions;
                        view.status.clear();
                    }
                    Some(Err(e)) => {
                        view.status =
                            format!("no se pudo conectar con titan-agentd: {}", error_text(&e))
                    }
                    None => {
                        view.status =
                            "no se pudo conectar con titan-agentd: conexión cerrada".into()
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
fn error_text(e: &ClientError) -> String {
    match e {
        ClientError::Io(e) => e.to_string(),
        ClientError::Rpc(e) => e.message.clone(),
        ClientError::Protocol(e) => e.clone(),
        ClientError::Closed => "conexión cerrada".into(),
        ClientError::Timeout => "tiempo de conexión agotado".into(),
    }
}
impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let counts = StateCounts::tally(self.sessions.iter().map(|s| s.state));
        let mut rows = div().flex().flex_col().gap_1();
        for group in build_tree(&self.sessions, &self.filter) {
            rows = rows.child(
                div()
                    .text_color(rgb(theme::INK))
                    .child(format!("[#] {}", group.project.name)),
            );
            for row in group.rows {
                let s = &self.sessions[row.session];
                let selected = self.selected.as_deref() == Some(s.id.as_str());
                rows = rows.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_color(rgb(theme::BODY))
                        .bg(rgb(if selected {
                            theme::SURFACE
                        } else {
                            theme::CANVAS
                        }))
                        .child(format!(
                            "{} {} {}  {}{}",
                            row.branch,
                            s.state.marker(),
                            s.name,
                            s.cli.tag(),
                            s.model
                                .as_ref()
                                .map(|m| format!(" {m}"))
                                .unwrap_or_default()
                        )),
                );
            }
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::CANVAS))
            .text_color(rgb(theme::BODY))
            .font_family("JetBrains Mono")
            .child(
                div()
                    .h(px(48.))
                    .border_b_1()
                    .border_color(gpui::rgba(theme::HAIRLINE_RGBA))
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .child("titan-agent  ")
                            .child(div().text_color(rgb(theme::MUTE)).child("torre de control")),
                    )
                    .child(format!(
                        "[>] {} trabajando   [?] {} te necesita   [+] {} en espera",
                        counts.working, counts.needs_you, counts.waiting
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .child(
                        div()
                            .w(px(320.))
                            .border_r_1()
                            .border_color(gpui::rgba(theme::HAIRLINE_RGBA))
                            .flex()
                            .flex_col()
                            .p_3()
                            .child(div().child("[/] ").child(self.filter.clone()))
                            .child(rows)
                            .child(div().mt_auto().text_color(rgb(theme::MUTE)).child(
                                "[>] trabajando  [?] te necesita  [+] en espera  [x] terminada",
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(theme::MUTE))
                            .child(if self.status.is_empty() {
                                "elige una sesión en la columna izquierda".to_owned()
                            } else {
                                self.status.clone()
                            }),
                    )
                    .child(
                        div()
                            .w(px(260.))
                            .border_l_1()
                            .border_color(gpui::rgba(theme::HAIRLINE_RGBA)),
                    ),
            )
    }
}
fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| {
                    cx.new(|_| MainWindow {
                        status: String::new(),
                        sessions: Vec::new(),
                        connection: None,
                        selected: None,
                        filter: String::new(),
                    })
                },
            )
            .expect("failed to open the main window");
        window.update(cx, |view, _, cx| view.start(cx)).ok();
        cx.activate(true);
    });
}
