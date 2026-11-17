//! Checks of the release archive (ADR-0011 §3, check 6): every entry stays
//! inside the one folder of the release, and the four binaries are there.

use super::{BINARIES, UpdateError, Version};

/// Kind of an archive entry, as far as the checks care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Hardlink,
    /// Devices, fifos and anything else: never extracted.
    Other,
}

/// The folder a release archive holds: `titan-agent-<v>-<target>`.
pub fn bundle_root(version: Version, target: &str) -> String {
    format!("titan-agent-{version}-{target}")
}

/// Checks one entry of the archive before it is extracted: `path` (and a
/// hard link's target) must be `root` or under it, relative and without
/// `..`; a symbolic link must point somewhere inside `root`.
pub fn check_entry(
    root: &str,
    path: &str,
    kind: EntryKind,
    link: Option<&str>,
) -> Result<(), UpdateError> {
    let unsafe_entry = || UpdateError::UnsafeEntry(path.to_string());
    let inside = components_inside(root, path).ok_or_else(unsafe_entry)?;
    match kind {
        EntryKind::File | EntryKind::Dir => Ok(()),
        EntryKind::Hardlink => {
            let target = link.ok_or_else(unsafe_entry)?;
            components_inside(root, target)
                .map(|_| ())
                .ok_or_else(unsafe_entry)
        }
        EntryKind::Symlink => {
            let target = link.ok_or_else(unsafe_entry)?;
            if target.is_empty() || target.starts_with('/') || target.contains('\\') {
                return Err(unsafe_entry());
            }
            // Resolve from the entry's folder, inside the root.
            let mut stack: Vec<&str> = inside[..inside.len().saturating_sub(1)].to_vec();
            for part in target.split('/') {
                match part {
                    "" | "." => {}
                    ".." => {
                        stack.pop().ok_or_else(unsafe_entry)?;
                    }
                    other => stack.push(other),
                }
            }
            Ok(())
        }
        EntryKind::Other => Err(unsafe_entry()),
    }
}

/// The components of `path` after `root`, if `path` is `root` or lies
/// under it with plain components.
fn components_inside<'a>(root: &str, path: &'a str) -> Option<Vec<&'a str>> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || root.is_empty() {
        return None;
    }
    let mut parts = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".");
    if parts.next()? != root {
        return None;
    }
    let rest: Vec<&str> = parts.collect();
    if rest.contains(&"..") {
        return None;
    }
    Some(rest)
}

/// `files` are the extracted files relative to the root (`bin/titan-agent`);
/// the four binaries must be among them.
pub fn check_bundle(files: &[&str]) -> Result<(), UpdateError> {
    for binary in BINARIES {
        let wanted = format!("bin/{binary}");
        if !files
            .iter()
            .any(|file| file.trim_start_matches("./") == wanted)
        {
            return Err(UpdateError::Incomplete(binary.to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_and_nested_links_are_judged_inside_the_root() {
        let ok = |path: &str, kind, link| check_entry("r", path, kind, link).is_ok();
        assert!(ok("./r/./bin/a", EntryKind::File, None));
        assert!(ok("r/a/b/c", EntryKind::Symlink, Some("../../x")));
        assert!(!ok("r/a/b/c", EntryKind::Symlink, Some("../../../x")));
        assert!(!ok("r/a", EntryKind::Symlink, None));
        assert!(!ok("r/a", EntryKind::Symlink, Some("")));
        assert!(!ok("rx/a", EntryKind::File, None));
    }
}
