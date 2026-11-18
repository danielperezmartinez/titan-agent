//! The imported projects of the daemon (docs/Decisiones/ADR-0014): the list
//! in `projects.json` of the settings folder, read once at start, written
//! atomically on every change, and never overwritten when it could not be
//! read. The list rules are in `titan_agent_core::projects`.

use std::path::{Path, PathBuf};

use titan_agent_core::project::project_name;
use titan_agent_core::projects::{
    ImportedProject, PROJECTS_FILE, add_project, initial_projects, parse_projects, remove_project,
    render_projects,
};
use titan_agent_core::protocol::ProjectInfo;
use titan_agent_core::session::Session;

/// An error for the client: its code and its text.
pub type Refusal = (&'static str, String);

pub struct ProjectStore {
    /// `projects.json`; `None` without a settings folder (no HOME).
    path: Option<PathBuf>,
    list: Vec<ImportedProject>,
    /// Why the file could not be read: it is then never written.
    unreadable: Option<String>,
    /// The file did not exist at start: the first discovery fills it.
    first: bool,
}

impl ProjectStore {
    /// Reads the list of `<config_dir>/projects.json`.
    pub fn load(config_dir: Option<&Path>) -> ProjectStore {
        let path = config_dir.map(|dir| dir.join(PROJECTS_FILE));
        let mut store = ProjectStore {
            path: path.clone(),
            list: Vec::new(),
            unreadable: None,
            first: false,
        };
        match path.as_deref().map(std::fs::read_to_string) {
            Some(Ok(text)) => match parse_projects(&text) {
                Ok(list) => store.list = list,
                Err(error) => store.unreadable = Some(error),
            },
            Some(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => store.first = true,
            Some(Err(error)) => {
                store.unreadable = Some(format!("no se pudo leer {PROJECTS_FILE}: {error}"))
            }
            None => {}
        }
        store
    }

    /// The first discovery of a start without the file: the projects that
    /// have sessions now. Once. Returns whether the list changed.
    pub fn first_discovery(&mut self, sessions: &[Session], now_ms: i64) -> bool {
        if !std::mem::take(&mut self.first) {
            return false;
        }
        let before = self.list.clone();
        for project in initial_projects(sessions, now_ms) {
            add_project(&mut self.list, &project.root, now_ms);
        }
        if let Err(error) = self.save() {
            eprintln!("titan-agentd: {error}");
        }
        self.list != before
    }

    /// `project_add`: an absolute path of an existing folder.
    pub fn add(&mut self, path: &str, now_ms: i64) -> Result<String, Refusal> {
        if !path.starts_with('/') || !Path::new(path).is_dir() {
            return Err((
                "not_a_folder",
                format!("no es una carpeta que exista: {path}"),
            ));
        }
        self.writable()?;
        let mut list = self.list.clone();
        let root = add_project(&mut list, path, now_ms);
        if list != self.list {
            self.commit(list)?;
        }
        Ok(root)
    }

    /// `project_remove`: forgets it, never touches its folder.
    pub fn remove(&mut self, root: &str) -> Result<(), Refusal> {
        self.writable()?;
        let mut list = self.list.clone();
        if remove_project(&mut list, root) {
            self.commit(list)?;
        }
        Ok(())
    }

    /// A launch in a folder: imports its project if it is not. Returns
    /// whether the list changed (a failure only goes to the log).
    pub fn import_launch(&mut self, cwd: &str, now_ms: i64) -> bool {
        if self.unreadable.is_some() {
            return false;
        }
        let mut list = self.list.clone();
        add_project(&mut list, cwd, now_ms);
        if list == self.list {
            return false;
        }
        match self.commit(list) {
            Ok(()) => true,
            Err((_, error)) => {
                eprintln!("titan-agentd: {error}");
                false
            }
        }
    }

    /// The list for the `sessions` event.
    pub fn infos(&self) -> Vec<ProjectInfo> {
        self.list
            .iter()
            .map(|p| ProjectInfo {
                root: p.root.clone(),
                name: project_name(&p.root),
                added_ms: p.added_ms,
                missing: !Path::new(&p.root).is_dir(),
            })
            .collect()
    }

    pub fn error(&self) -> Option<String> {
        self.unreadable.clone()
    }

    /// Roots that exist, where team tasks are also looked for.
    pub fn roots(&self) -> Vec<String> {
        self.list
            .iter()
            .filter(|p| Path::new(&p.root).is_dir())
            .map(|p| p.root.clone())
            .collect()
    }

    fn writable(&self) -> Result<(), Refusal> {
        match &self.unreadable {
            Some(error) => Err(("config_error", format!("{error}; no se toca el fichero"))),
            None => Ok(()),
        }
    }

    fn commit(&mut self, list: Vec<ImportedProject>) -> Result<(), Refusal> {
        let previous = std::mem::replace(&mut self.list, list);
        self.save().map_err(|error| {
            self.list = previous;
            ("config_error", error)
        })
    }

    /// Writes to a temporary file in the same folder and renames it over
    /// the list, so it is never left half written.
    fn save(&self) -> Result<(), String> {
        let path = self
            .path
            .as_ref()
            .ok_or("no hay carpeta de configuración")?;
        let dir = path.parent().ok_or("carpeta de configuración no válida")?;
        let fail = |error: std::io::Error| format!("no se pudo escribir {PROJECTS_FILE}: {error}");
        std::fs::create_dir_all(dir).map_err(fail)?;
        let tmp = dir.join(format!(".{PROJECTS_FILE}.{}.tmp", std::process::id()));
        std::fs::write(&tmp, render_projects(&self.list)).map_err(fail)?;
        std::fs::rename(&tmp, path).map_err(|error| {
            let _ = std::fs::remove_file(&tmp);
            fail(error)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "titan-agentd-projects-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_launch_imports_its_project_and_a_bad_file_is_kept() {
        let dir = folder("launch");
        let mut store = ProjectStore::load(Some(&dir));
        assert!(store.first_discovery(&[], 1) || store.infos().is_empty());
        assert!(store.import_launch("/srv/demo/.agents/worktrees/x", 2));
        assert!(!store.import_launch("/srv/demo", 3));
        let again = ProjectStore::load(Some(&dir));
        assert_eq!(again.infos()[0].root, "/srv/demo");
        assert!(again.infos()[0].missing);

        std::fs::write(dir.join(PROJECTS_FILE), "{roto").unwrap();
        let mut bad = ProjectStore::load(Some(&dir));
        assert!(bad.error().is_some());
        assert!(!bad.import_launch("/srv/otro", 4));
        assert_eq!(bad.remove("/srv/demo").unwrap_err().0, "config_error");
        assert_eq!(
            std::fs::read_to_string(dir.join(PROJECTS_FILE)).unwrap(),
            "{roto"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
