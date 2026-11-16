//! titan-team: the team tool of titan-agent (ADR-0005), the Rust rewrite of
//! the Python prototype `bin/team`.
//!
//! The director (Claude Code) runs it with Bash to hand work to the roles of
//! `.agents/team.toml`, each one an official `codex exec` thread in a worktree
//! of its own task. Every step is written to the event log of ADR-0010, which
//! titan-agentd reads for the project monitor. It never launches Claude Code.

mod args;
mod ask;
mod codex;
mod commands;
mod context;
mod git;
mod log;
mod state;

use std::process::ExitCode;

use args::Command;

/// An error for the user, in Spanish, and the exit code it ends with.
#[derive(Debug)]
pub struct TeamError {
    pub message: String,
    pub code: u8,
}

impl TeamError {
    pub fn new(message: impl Into<String>) -> TeamError {
        TeamError {
            message: message.into(),
            code: 2,
        }
    }

    pub fn with_code(message: impl Into<String>, code: u8) -> TeamError {
        TeamError {
            message: message.into(),
            code,
        }
    }
}

pub type Result<T> = std::result::Result<T, TeamError>;

fn main() -> ExitCode {
    let parsed = match args::parse(std::env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{}", error.message);
            return ExitCode::from(error.code);
        }
    };
    let result = match parsed.command {
        Command::Help => {
            print!("{}", args::USAGE);
            Ok(0)
        }
        Command::Version => {
            println!("titan-team {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Command::Doc => {
            print!("{}", include_str!("../FORMAT.md"));
            Ok(0)
        }
        Command::Init { force } => commands::init(parsed.config.as_deref(), force),
        command => context::Context::load(parsed.config.as_deref())
            .and_then(|ctx| commands::run(&ctx, command)),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{}", error.message);
            ExitCode::from(error.code)
        }
    }
}
