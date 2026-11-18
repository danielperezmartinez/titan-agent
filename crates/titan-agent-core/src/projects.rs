//! The imported projects (docs/Decisiones/ADR-0014): `projects.json` v1 in
//! the settings folder of titan-agent, kept by the daemon. Pure: reading,
//! writing the text and the list rules; the daemon does the file I/O.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::project::project_root;
use crate::session::Session;

pub const PROJECTS_FILE: &str = "projects.json";
pub const PROJECTS_VERSION: u32 = 1;

/// One imported project: its root (as [`project_root`] gives it) and when
/// it was imported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedProject {
    pub root: String,
    pub added_ms: i64,
}

#[derive(Serialize)]
struct FileV1<'a> {
    version: u32,
    projects: &'a [ImportedProject],
}

/// A root without its trailing slashes (except `/` itself).
fn trimmed(root: &str) -> &str {
    let trimmed = root.trim_end_matches('/');
    if trimmed.is_empty() && root.starts_with('/') {
        "/"
    } else {
        trimmed
    }
}

/// Reads `projects.json`. Anything but a well-formed v1 file is an error,
/// so the daemon never overwrites a file it does not understand.
pub fn parse_projects(text: &str) -> Result<Vec<ImportedProject>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("{PROJECTS_FILE} no es JSON: {e}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("{PROJECTS_FILE} no es un objeto"))?;
    match object.get("version").and_then(Value::as_u64) {
        Some(v) if v == u64::from(PROJECTS_VERSION) => {}
        Some(v) => return Err(format!("{PROJECTS_FILE} es de la versión {v}")),
        None => return Err(format!("{PROJECTS_FILE} no dice su versión")),
    }
    let entries = object
        .get("projects")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{PROJECTS_FILE} no tiene la lista projects"))?;
    let mut projects: Vec<ImportedProject> = Vec::new();
    for entry in entries {
        let root = entry.get("root").and_then(Value::as_str);
        let added_ms = entry.get("added_ms").and_then(Value::as_i64);
        let (Some(root), Some(added_ms)) = (root, added_ms) else {
            return Err(format!("{PROJECTS_FILE}: entrada no válida: {entry}"));
        };
        let root = trimmed(root).to_string();
        if !projects.iter().any(|p| p.root == root) {
            projects.push(ImportedProject { root, added_ms });
        }
    }
    Ok(projects)
}

/// The v1 text of the list, ending in a line break.
pub fn render_projects(projects: &[ImportedProject]) -> String {
    let file = FileV1 {
        version: PROJECTS_VERSION,
        projects,
    };
    let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
    text.push('\n');
    text
}

/// Imports the project of `path` (worktrees fold into their repository) at
/// the end of the list, unless it is there. Returns its root.
pub fn add_project(projects: &mut Vec<ImportedProject>, path: &str, now_ms: i64) -> String {
    let root = project_root(path);
    if !projects.iter().any(|p| p.root == root) {
        projects.push(ImportedProject {
            root: root.clone(),
            added_ms: now_ms,
        });
    }
    root
}

/// Forgets a project (never its folder). Returns whether it was there.
pub fn remove_project(projects: &mut Vec<ImportedProject>, root: &str) -> bool {
    let root = trimmed(root);
    let before = projects.len();
    projects.retain(|p| p.root != root);
    projects.len() != before
}

/// The first list: the projects that already have sessions.
pub fn initial_projects(sessions: &[Session], now_ms: i64) -> Vec<ImportedProject> {
    let mut roots: Vec<String> = sessions.iter().map(|s| s.project_root.clone()).collect();
    roots.sort();
    roots.dedup();
    roots
        .into_iter()
        .map(|root| ImportedProject {
            root,
            added_ms: now_ms,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_what_it_understands() {
        let list = parse_projects(
            r#"{"version":1,"x":true,"projects":[{"root":"/a/","added_ms":1},{"root":"/a","added_ms":2},{"root":"/","added_ms":3}]}"#,
        )
        .unwrap();
        assert_eq!(list.iter().map(|p| p.root.as_str()).collect::<Vec<_>>(), ["/a", "/"]);
        assert_eq!(parse_projects(&render_projects(&list)), Ok(list));
        for bad in [
            "",
            "[]",
            r#"{"projects":[]}"#,
            r#"{"version":2,"projects":[]}"#,
            r#"{"version":1}"#,
            r#"{"version":1,"projects":[{"root":"/a"}]}"#,
        ] {
            assert!(parse_projects(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn adds_once_and_removes() {
        let mut list = Vec::new();
        assert_eq!(add_project(&mut list, "/p/.claude/worktrees/x/sub", 5), "/p");
        assert_eq!(add_project(&mut list, "/p/", 9), "/p");
        assert_eq!(list, [ImportedProject { root: "/p".into(), added_ms: 5 }]);
        assert!(!remove_project(&mut list, "/q"));
        assert!(remove_project(&mut list, "/p/"));
        assert!(list.is_empty());
    }
}
