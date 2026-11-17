//! The update notice `[^]` in the header and the «Actualizaciones» block of
//! Configuración (docs/Decisiones visuales/Aviso de actualización en la
//! cabecera.md, ADR-0011 §6–§7). The daemon checks, downloads and applies;
//! this view only shows its state and sends the user's clicks. After
//! «actualizar» it reopens through the launcher, and it confirms a new
//! version by deleting `update/pending.json` once it has run 10 s against a
//! daemon of its own version.

use std::path::PathBuf;
use std::time::Duration;

use gpui::{ClickEvent, Context, Div, FontWeight, Role, SharedString, div, prelude::*, px};
use titan_agent_core::protocol::{Request, UpdateState, UpdateStatus};
use titan_agent_core::update::install_root;

use crate::theme::{self, color, hairline};

use super::{MainView, Pending};

/// Below this window width the notice shrinks to `[^] actualizar`.
const NARROW_W: f32 = 1100.;
/// Width of the labels of Configuración (the same in every block).
pub(super) const LABEL_W: f32 = 184.;
/// A new version is confirmed after this long against its own daemon.
const CONFIRM_AFTER: Duration = Duration::from_secs(10);

/// What the view knows about updates.
#[derive(Default)]
pub(super) struct UpdateUi {
    pub status: Option<UpdateStatus>,
    /// Version the daemon gave in `hello`.
    pub daemon: Option<String>,
    /// The inline confirmation is open.
    pub confirm: bool,
    /// Screenshots only (`TITAN_AGENT_DEMO_UPDATE`): a fixed status.
    demo: Option<(UpdateStatus, Option<String>, bool)>,
}

/// One line of the notice, as the visual decision lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Notice {
    Downloading(String),
    Ready { version: String, short: bool },
    Confirm { sessions: usize },
    Applying,
    Failed(Option<String>),
    RolledBack { from: String, to: String },
    OtherDaemon(String),
}

/// The notice for `status`, or none. `ui` is the version of this window.
pub(super) fn notice(
    status: &UpdateStatus,
    daemon: Option<&str>,
    ui: &str,
    confirm: bool,
    narrow: bool,
) -> Option<Notice> {
    if let Some(back) = &status.rolled_back {
        return Some(Notice::RolledBack {
            from: back.from.clone(),
            to: back.to.clone(),
        });
    }
    let version = status.version.clone().unwrap_or_default();
    match status.state {
        UpdateState::Applying => return Some(Notice::Applying),
        UpdateState::Ready | UpdateState::Waiting if confirm => {
            return Some(Notice::Confirm {
                sessions: status.blocking.len(),
            });
        }
        UpdateState::Ready | UpdateState::Waiting => {
            return Some(Notice::Ready {
                version,
                short: narrow,
            });
        }
        UpdateState::Downloading => return Some(Notice::Downloading(version)),
        UpdateState::Failed => return Some(Notice::Failed(status.version.clone())),
        UpdateState::Idle | UpdateState::Checking => {}
    }
    match daemon {
        Some(daemon) if status.active && daemon != ui => Some(Notice::OtherDaemon(daemon.into())),
        _ => None,
    }
}

impl UpdateUi {
    pub fn new() -> UpdateUi {
        let mut ui = UpdateUi::default();
        if let Ok(scene) = std::env::var("TITAN_AGENT_DEMO_UPDATE") {
            ui.demo = demo(&scene);
        }
        ui
    }

    /// The status shown: the daemon's, or the demo's.
    pub fn shown(&self) -> (Option<&UpdateStatus>, Option<&str>, bool) {
        match &self.demo {
            Some((status, daemon, confirm)) => (Some(status), daemon.as_deref(), *confirm),
            None => (self.status.as_ref(), self.daemon.as_deref(), self.confirm),
        }
    }
}

/// Fixed states for the screenshots of the demo kit.
fn demo(scene: &str) -> Option<(UpdateStatus, Option<String>, bool)> {
    let mut status = UpdateStatus {
        active: true,
        installed: env!("CARGO_PKG_VERSION").into(),
        current: Some(env!("CARGO_PKG_VERSION").into()),
        version: Some("0.6.1".into()),
        notes_url: Some(
            "https://github.com/danielperezmartinez/titan-agent/releases/tag/v0.6.1".into(),
        ),
        auto: true,
        last_check_ms: Some(super::now_ms() - 3 * 60_000),
        ..UpdateStatus::default()
    };
    let mut daemon = Some(env!("CARGO_PKG_VERSION").to_string());
    let mut confirm = false;
    match scene {
        "ready" => status.state = UpdateState::Ready,
        "downloading" => status.state = UpdateState::Downloading,
        "confirm" => {
            status.state = UpdateState::Ready;
            status.blocking = vec!["cl:a".into(), "cx:b".into()];
            confirm = true;
        }
        "applying" => status.state = UpdateState::Applying,
        "failed" => {
            status.state = UpdateState::Failed;
            status.error = Some("el SHA-256 de la descarga no coincide".into());
        }
        "rollback" => {
            status.version = None;
            status.rolled_back = Some(titan_agent_core::update::RolledBack {
                from: env!("CARGO_PKG_VERSION").into(),
                to: "0.6.1".into(),
            });
        }
        "daemon" => {
            status.version = None;
            daemon = Some("0.5.2".into());
        }
        _ => return None,
    }
    Some((status, daemon, confirm))
}

impl MainView {
    /// The daemon answered `hello`: ask for the updater's state and, in an
    /// installed version, confirm it after a while.
    pub(super) fn update_hello(&mut self, daemon: String, cx: &mut Context<Self>) {
        let same = daemon == env!("CARGO_PKG_VERSION");
        self.update.daemon = Some(daemon);
        let id = self.client.request(Request::UpdateStatus);
        self.pending.insert(id, Pending::Update);
        if !same {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CONFIRM_AFTER).await;
            let still = this
                .update(cx, |view, _| {
                    view.connected
                        && view.update.daemon.as_deref() == Some(env!("CARGO_PKG_VERSION"))
                })
                .unwrap_or(false);
            if still {
                confirm_version();
            }
        })
        .detach();
    }

    /// An answer to an `update_*` request.
    pub(super) fn update_answer(&mut self, result: serde_json::Value) {
        if let Ok(status) = serde_json::from_value::<UpdateStatus>(result) {
            self.update.status = Some(status);
        }
    }

    /// `update_apply` succeeded: the daemon is about to replace itself.
    /// Reopen through the launcher and close this window.
    pub(super) fn update_applied(&mut self, cx: &mut Context<Self>) {
        self.update.confirm = false;
        if let Some(launcher) = std::env::var_os("TITAN_AGENT_LAUNCHER").map(PathBuf::from) {
            relaunch(&launcher);
            cx.quit();
        }
    }

    /// «actualizar»: asks first if terminals of working sessions would close.
    fn update_click(&mut self, cx: &mut Context<Self>) {
        let blocking = self
            .update
            .status
            .as_ref()
            .is_some_and(|s| !s.blocking.is_empty());
        if blocking && !self.update.confirm {
            self.update.confirm = true;
        } else {
            self.update_send(Request::UpdateApply {
                force: self.update.confirm,
            });
        }
        cx.notify();
    }

    fn update_send(&mut self, request: Request) {
        let pending = match request {
            Request::UpdateApply { .. } => Pending::UpdateApply,
            _ => Pending::Update,
        };
        let id = self.client.request(request);
        self.pending.insert(id, pending);
    }

    /// Esc closes the confirmation.
    pub(super) fn update_cancel(&mut self) -> bool {
        std::mem::take(&mut self.update.confirm)
    }

    /// The notice in the header, between the counters and `[i]` `[*]`.
    pub(super) fn update_notice(&self, cx: &mut Context<Self>) -> Option<Div> {
        let (status, daemon, confirm) = self.update.shown();
        let narrow = self.viewport_w < NARROW_W;
        let notice = notice(status?, daemon, env!("CARGO_PKG_VERSION"), confirm, narrow)?;
        let notes = status.and_then(|s| s.notes_url.clone());
        let mute = |text: String| div().text_color(color(theme::MUTE)).child(text);
        let marker = |warn: bool| {
            div()
                .text_color(color(if warn { theme::WARNING } else { theme::MUTE }))
                .child("[^]")
        };
        let link = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .text_color(color(theme::ACCENT))
                .cursor_pointer()
                .child(label)
        };
        let version_link = |version: String| {
            div()
                .id("update-notes")
                .text_color(color(theme::MUTE))
                .cursor_pointer()
                .hover(|d| d.underline())
                .child(format!("v{version}"))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    if let Some(url) = &notes {
                        cx.open_url(url);
                    }
                }))
        };
        let row = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .min_w(px(0.))
            .text_size(px(12.))
            .whitespace_nowrap();
        Some(match notice {
            Notice::Ready { version, short } => {
                let row = row.child(marker(false));
                let row = if short {
                    row
                } else {
                    row.child(version_link(version)).child(mute("·".into()))
                };
                row.child(
                    link("update-apply", "actualizar")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.update_click(cx))),
                )
            }
            Notice::Downloading(version) => row
                .child(marker(false))
                .child(mute(format!("v{version} · descargando"))),
            Notice::Confirm { sessions } => row
                .flex_shrink_1()
                .child(marker(false))
                .child(
                    div()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(color(theme::BODY))
                        .child("se cerrarán los terminales de ")
                        .child(
                            div()
                                .text_color(color(theme::WARNING))
                                .child(format!("{sessions} {}", sessions_word(sessions))),
                        )
                        .child("; siguen vivas")
                        .flex(),
                )
                .child(mute("·".into()))
                .child(
                    div()
                        .id("update-confirm")
                        .role(Role::Button)
                        .aria_label("actualizar")
                        .flex_none()
                        .px(px(8.))
                        .py(px(3.))
                        .border_1()
                        .border_color(color(theme::ACCENT))
                        .rounded(px(4.))
                        .bg(color(theme::SURFACE))
                        .text_color(color(theme::INK))
                        .cursor_pointer()
                        .child("[ok] actualizar")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.update_click(cx))),
                )
                .child(mute("·".into()))
                .child(
                    div()
                        .id("update-cancel")
                        .role(Role::Button)
                        .flex_none()
                        .text_color(color(theme::MUTE))
                        .cursor_pointer()
                        .child("cancelar")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.update.confirm = false;
                            cx.notify();
                        })),
                ),
            Notice::Applying => row.child(marker(false)).child(mute("actualizando…".into())),
            Notice::Failed(version) => row.child(marker(false)).child(mute(match version {
                Some(v) => format!("v{v} no se pudo verificar"),
                None => "la actualización no se pudo verificar".into(),
            })),
            Notice::RolledBack { from, to } => row
                .child(marker(true))
                .child(mute(format!("v{to} no arrancó; sigues en v{from}")))
                .child(
                    div()
                        .id("update-dismiss")
                        .role(Role::Button)
                        .aria_label("descartar el aviso")
                        .text_color(color(theme::BODY))
                        .cursor_pointer()
                        .child("[x]")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.update_send(Request::UpdateDismiss);
                            if let Some(status) = this.update.status.as_mut() {
                                status.rolled_back = None;
                            }
                            cx.notify();
                        })),
                ),
            Notice::OtherDaemon(version) => row
                .child(marker(false))
                .child(mute(format!("daemon v{version}")))
                .child(mute("·".into()))
                .child(
                    link("update-restart", "reiniciar")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.update_click(cx))),
                ),
        })
    }

    /// «ACTUALIZACIONES» in Configuración.
    pub(super) fn update_settings(&self, cx: &mut Context<Self>) -> Div {
        let (status, _, _) = self.update.shown();
        let label = |text: &'static str| {
            div()
                .w(px(LABEL_W))
                .flex_none()
                .text_size(px(12.))
                .text_color(color(theme::MUTE))
                .child(text)
        };
        let chip = |id: &'static str, text: SharedString| {
            div()
                .id(id)
                .role(Role::Button)
                .flex()
                .items_center()
                .min_h(px(32.))
                .px(px(8.))
                .border_1()
                .border_color(hairline())
                .rounded(px(4.))
                .text_size(px(12.))
                .text_color(color(theme::BODY))
                .cursor_pointer()
                .child(text)
        };
        let active = status.is_some_and(|s| s.active);
        let auto = status.is_none_or(|s| s.auto);
        let mut block = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .border_1()
            .border_color(hairline())
            .rounded(px(4.))
            .bg(color(theme::CANVAS))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(color(theme::INK))
                    .child("ACTUALIZACIONES"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(label("versión instalada"))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(color(theme::BODY))
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            );
        if active {
            block = block.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(label("buscar actualizaciones"))
                    .child(
                        chip(
                            "update-auto",
                            if auto {
                                "[ok] activado".into()
                            } else {
                                "[ ] desactivado".into()
                            },
                        )
                        .aria_label("buscar actualizaciones")
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                this.update_send(Request::UpdateSettings { auto: !auto });
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        chip("update-check", "Buscar ahora".into()).on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.update_send(Request::UpdateCheck);
                                cx.notify();
                            },
                        )),
                    ),
            );
        }
        block.child(
            div()
                .pl(px(if active { LABEL_W + 4. } else { 0. }))
                .text_size(px(11.))
                .text_color(color(theme::MUTE))
                .child(last_check_line(status, self.now_ms)),
        )
    }
}

fn sessions_word(n: usize) -> &'static str {
    if n == 1 {
        "sesión trabajando"
    } else {
        "sesiones trabajando"
    }
}

/// The `mute` line under the switch: when it last looked and what it found.
pub(super) fn last_check_line(status: Option<&UpdateStatus>, now_ms: i64) -> String {
    let Some(status) = status.filter(|s| s.active) else {
        return "versión de desarrollo: no busca actualizaciones".into();
    };
    let when = match status.last_check_ms {
        Some(at) => format!(
            "última comprobación: {}",
            titan_agent_core::time::ago(at, now_ms)
        ),
        None => "aún no ha comprobado".into(),
    };
    let what = match (status.state, &status.version, &status.error) {
        (UpdateState::Checking, ..) => "comprobando…".to_string(),
        (UpdateState::Downloading, Some(v), _) => format!("descargando la versión v{v}"),
        (UpdateState::Ready | UpdateState::Waiting, Some(v), _) => {
            format!("hay una versión nueva (v{v}), se aplicará al cerrar la ventana")
        }
        (_, _, Some(error)) => format!("error: {error}"),
        _ => "estás en la última versión".into(),
    };
    format!("{when} · {what}")
}

/// Deletes `update/pending.json` of the install this window runs from: the
/// new version started and talked to its own daemon.
fn confirm_version() {
    let Some((root, _)) = std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(install_root)
    else {
        return;
    };
    let _ = std::fs::remove_file(root.join("update/pending.json"));
}

/// Starts the launcher a moment later (the daemon replaces itself first),
/// detached from this window.
fn relaunch(launcher: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 1.5; exec \"$0\"")
            .arg(launcher)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn();
    }
    #[cfg(not(unix))]
    let _ = launcher;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: UpdateState) -> UpdateStatus {
        UpdateStatus {
            state,
            active: true,
            version: Some("0.6.1".into()),
            ..UpdateStatus::default()
        }
    }

    #[test]
    fn each_state_has_its_line_and_nothing_shows_when_idle() {
        let n =
            |s: &UpdateStatus, confirm, narrow| notice(s, Some("0.6.0"), "0.6.0", confirm, narrow);
        assert_eq!(n(&status(UpdateState::Idle), false, false), None);
        assert_eq!(n(&status(UpdateState::Checking), false, false), None);
        assert_eq!(
            n(&status(UpdateState::Waiting), false, true),
            Some(Notice::Ready {
                version: "0.6.1".into(),
                short: true
            })
        );
        let mut ready = status(UpdateState::Ready);
        ready.blocking = vec!["cl:a".into(), "cl:b".into()];
        assert_eq!(
            n(&ready, true, false),
            Some(Notice::Confirm { sessions: 2 })
        );
        assert_eq!(
            n(&status(UpdateState::Applying), true, false),
            Some(Notice::Applying)
        );
        assert_eq!(
            n(&status(UpdateState::Downloading), false, false),
            Some(Notice::Downloading("0.6.1".into()))
        );
        let mut back = status(UpdateState::Ready);
        back.rolled_back = Some(titan_agent_core::update::RolledBack {
            from: "0.6.0".into(),
            to: "0.6.1".into(),
        });
        assert!(matches!(
            n(&back, false, false),
            Some(Notice::RolledBack { .. })
        ));
    }

    #[test]
    fn another_daemon_version_only_shows_in_an_install() {
        let idle = status(UpdateState::Idle);
        assert_eq!(
            notice(&idle, Some("0.5.2"), "0.6.0", false, false),
            Some(Notice::OtherDaemon("0.5.2".into()))
        );
        let dev = UpdateStatus {
            active: false,
            ..idle
        };
        assert_eq!(notice(&dev, Some("0.5.2"), "0.6.0", false, false), None);
    }

    #[test]
    fn the_settings_line_says_what_the_last_check_found() {
        assert_eq!(
            last_check_line(None, 0),
            "versión de desarrollo: no busca actualizaciones"
        );
        let mut s = status(UpdateState::Ready);
        s.last_check_ms = Some(0);
        assert!(last_check_line(Some(&s), 1000).ends_with("se aplicará al cerrar la ventana"));
        s.state = UpdateState::Idle;
        s.version = None;
        s.error = Some("sin conexión".into());
        assert!(last_check_line(Some(&s), 1000).ends_with("error: sin conexión"));
    }
}
