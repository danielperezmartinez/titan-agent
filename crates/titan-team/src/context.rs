//! Where the team lives: the `team.toml` found from the current folder, the
//! folder that holds `.agents/` and the state folder.

use std::path::{Component, Path, PathBuf};

use titan_agent_core::team::config::TeamConfig;

use crate::{Result, TeamError};

pub struct Context {
    pub config: TeamConfig,
    pub config_path: PathBuf,
    /// Folder that holds `.agents/`; relative paths of `team.toml` start here.
    pub base: PathBuf,
    pub state_dir: PathBuf,
    /// Claude Code session that runs the tool (the director), if any.
    pub director: Option<String>,
}

impl Context {
    pub fn load(explicit: Option<&str>) -> Result<Context> {
        let path = find_config(explicit)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| TeamError::new(format!("No se pudo leer {}: {e}", path.display())))?;
        let config = TeamConfig::parse(&text)
            .map_err(|e| TeamError::new(format!("{e}\n(fichero: {})", path.display())))?;
        let base = path
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .ok_or_else(|| TeamError::new("team.toml debe estar en <carpeta>/.agents/"))?;
        let state_dir = normalize(&base.join(&config.state_dir));
        Ok(Context {
            config,
            config_path: path,
            base,
            state_dir,
            director: std::env::var("CLAUDE_CODE_SESSION_ID")
                .ok()
                .filter(|id| !id.trim().is_empty()),
        })
    }

    /// A path of `team.toml` (or of the task state), relative to the base.
    pub fn path(&self, value: &str) -> PathBuf {
        let path = Path::new(value);
        if path.is_absolute() {
            normalize(path)
        } else {
            normalize(&self.base.join(path))
        }
    }

    /// `path` relative to the base when it is inside it, for the state file.
    pub fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.base)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned())
    }

    pub fn repo(&self) -> PathBuf {
        self.path(&self.config.project.repo)
    }
}

/// `--config` (a file or a folder with `.agents/team.toml`), or the first
/// `.agents/team.toml` from the current folder up.
pub fn find_config(explicit: Option<&str>) -> Result<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|e| TeamError::new(format!("No se puede leer la carpeta actual: {e}")))?;
    if let Some(explicit) = explicit {
        let mut path = normalize(&cwd.join(explicit));
        if path.is_dir() {
            path = path.join(".agents/team.toml");
        }
        if !path.is_file() {
            return Err(TeamError::new(format!(
                "No existe la configuración: {}",
                path.display()
            )));
        }
        return Ok(path);
    }
    for dir in cwd.ancestors() {
        let candidate = dir.join(".agents/team.toml");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(TeamError::new(
        "No se encuentra .agents/team.toml; usa --config RUTA o crea uno con titan-team init",
    ))
}

/// Resolves `.` and `..` without touching the disk (the worktree may not
/// exist yet).
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Current time as ISO 8601 in UTC, like the prototype's state files.
pub fn now_iso() -> String {
    let ms = titan_agent_core::time::now_ms();
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}+00:00",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_773), (2026, 11, 16));
        assert_eq!(civil(11_016), (2000, 2, 29));
    }

    #[test]
    fn normalize_drops_dots() {
        assert_eq!(
            normalize(Path::new("/a/b/./c/../d")),
            PathBuf::from("/a/b/d")
        );
    }
}
