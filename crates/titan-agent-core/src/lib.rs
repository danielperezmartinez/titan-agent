//! Shared protocol types and the session, project and event model of
//! titan-agent. It must not depend on GPUI, the daemon or platform I/O.

pub mod discovery;
pub mod project;
pub mod protocol;
pub mod session;
pub mod time;
pub mod tree;
