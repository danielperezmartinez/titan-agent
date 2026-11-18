//! Shared protocol types and the session, project and event model of
//! titan-agent. It must not depend on GPUI, the daemon or platform I/O.

pub mod discovery;
pub mod launch;
pub mod monitor;
pub mod project;
pub mod projects;
pub mod protocol;
pub mod quota;
pub mod session;
pub mod team;
pub mod time;
pub mod transcript;
pub mod tree;
pub mod update;
