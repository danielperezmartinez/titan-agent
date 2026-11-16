//! Calls to git.

use std::path::Path;
use std::process::{Command, Output};

use crate::{Result, TeamError};

/// Runs `git -C <repo> <args>` and fails with git's own message.
pub fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = run(repo, args)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(TeamError::new(if detail.is_empty() {
            format!("git terminó con {}", output.status)
        } else {
            detail
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs git without failing on a non-zero exit.
pub fn run(repo: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| TeamError::new(format!("git no está disponible en PATH: {e}")))
}

/// Whether git has a user name and email for commits in `repo`.
pub fn has_identity(repo: &Path) -> bool {
    ["user.name", "user.email"]
        .iter()
        .all(|key| run(repo, &["config", key]).is_ok_and(|o| o.status.success()))
}
