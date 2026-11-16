//! Appends events to `<state_dir>/events.jsonl` (ADR-0010). A failure to
//! write the log never stops the team: it is reported once on stderr.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use titan_agent_core::team::events::{EVENTS_FILE, EventKind, TeamEvent};

use crate::context::Context;

#[derive(Clone)]
pub struct EventLog {
    path: PathBuf,
    task: String,
    director: Option<String>,
}

impl EventLog {
    pub fn new(ctx: &Context, task: &str) -> EventLog {
        EventLog {
            path: ctx.state_dir.join(EVENTS_FILE),
            task: task.to_string(),
            director: ctx.director.clone(),
        }
    }

    pub fn emit(&self, kind: EventKind) {
        let line = TeamEvent::new(&self.task, self.director.clone(), kind).to_line();
        let written = self
            .path
            .parent()
            .map(std::fs::create_dir_all)
            .unwrap_or(Ok(()))
            .and_then(|_| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.path)
            })
            // One write per line on an append-only file.
            .and_then(|mut file| file.write_all(line.as_bytes()));
        if let Err(error) = written {
            eprintln!(
                "aviso: no se pudo escribir el evento en {}: {error}",
                self.path.display()
            );
        }
    }
}
