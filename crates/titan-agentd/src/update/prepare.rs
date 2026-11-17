//! Preparing a version: download its archive to `update/`, verify it and
//! extract it, entry by entry after its checks, to `versions/<v>` through a
//! temporary folder and a rename. `current` does not change here.

use std::path::{Path, PathBuf};

use tar::EntryType;
use titan_agent_core::update::{
    BINARIES, EntryKind, LINUX_TARGET, Selected, UpdateError, Version, bundle_root, check_bundle,
    check_entry,
};

use super::fetch;

/// Whether `versions/<v>` is already there with its four binaries.
pub fn is_prepared(root: &Path, version: Version) -> bool {
    let bin = root.join("versions").join(version.to_string()).join("bin");
    BINARIES.iter().all(|name| bin.join(name).is_file())
}

pub fn prepare(root: &Path, selected: &Selected, agent: ureq::Agent) -> Result<(), UpdateError> {
    if is_prepared(root, selected.version) {
        return Ok(());
    }
    let io = |path: &Path, error: std::io::Error| {
        UpdateError::Io(format!("{}: {error}", path.display()))
    };
    let update = root.join("update");
    std::fs::create_dir_all(&update).map_err(|e| io(&update, e))?;
    let part = update.join(format!("{}.part", selected.artifact.name));
    let downloaded = fetch::download(&agent, &selected.url, &selected.artifact, &part);
    let extracted = downloaded.and_then(|()| extract(&part, root, selected.version));
    let _ = std::fs::remove_file(&part);
    extracted
}

/// Extracts the archive at `archive` into `root/versions/<version>`.
pub fn extract(archive: &Path, root: &Path, version: Version) -> Result<(), UpdateError> {
    let versions = root.join("versions");
    let io = |path: &Path, error: std::io::Error| {
        UpdateError::Io(format!("{}: {error}", path.display()))
    };
    let temp: PathBuf = versions.join(format!(".prepare-{version}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).map_err(|e| io(&temp, e))?;
    let result = (|| {
        let name = bundle_root(version, LINUX_TARGET);
        let file = std::fs::File::open(archive).map_err(|e| io(archive, e))?;
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        tar.set_preserve_permissions(true);
        tar.set_overwrite(false);
        let mut files: Vec<String> = Vec::new();
        let broken = |e: std::io::Error| UpdateError::UnsafeEntry(format!("archivo dañado: {e}"));
        for entry in tar.entries().map_err(broken)? {
            let mut entry = entry.map_err(broken)?;
            let kind = match entry.header().entry_type() {
                EntryType::Regular | EntryType::Continuous => EntryKind::File,
                EntryType::Directory => EntryKind::Dir,
                EntryType::Symlink => EntryKind::Symlink,
                EntryType::Link => EntryKind::Hardlink,
                // Extension headers only describe the next entry.
                EntryType::XGlobalHeader
                | EntryType::XHeader
                | EntryType::GNULongName
                | EntryType::GNULongLink => continue,
                _ => EntryKind::Other,
            };
            let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).ok();
            let path = text(&entry.path_bytes())
                .ok_or_else(|| UpdateError::UnsafeEntry("ruta que no es UTF-8".into()))?;
            let link = match entry.link_name_bytes() {
                Some(bytes) => Some(
                    text(&bytes)
                        .ok_or_else(|| UpdateError::UnsafeEntry(format!("{path}: enlace")))?,
                ),
                None => None,
            };
            check_entry(&name, &path, kind, link.as_deref())?;
            if kind == EntryKind::File
                && let Some(rest) = path
                    .trim_start_matches("./")
                    .strip_prefix(&format!("{name}/"))
            {
                files.push(rest.to_string());
            }
            if !entry.unpack_in(&temp).map_err(broken)? {
                return Err(UpdateError::UnsafeEntry(path));
            }
        }
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        check_bundle(&refs)?;
        let target = versions.join(version.to_string());
        std::fs::rename(temp.join(&name), &target).map_err(|e| io(&target, e))
    })();
    let _ = std::fs::remove_dir_all(&temp);
    result
}
