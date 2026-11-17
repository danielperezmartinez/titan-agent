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
                // A sparse file is a regular file stored with its holes.
                EntryType::Regular | EntryType::Continuous | EntryType::GNUSparse => {
                    EntryKind::File
                }
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
                return Err(UpdateError::UnsafeEntry(format!("{path} (al extraer)")));
            }
        }
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        check_bundle(&refs)?;
        let target = versions.join(version.to_string());
        let extracted = temp.join(&name);
        if !std::fs::symlink_metadata(&extracted).is_ok_and(|m| m.is_dir()) {
            return Err(UpdateError::UnsafeEntry(format!(
                "{name} no es una carpeta"
            )));
        }
        std::fs::rename(&extracted, &target).map_err(|e| io(&target, e))
    })();
    let _ = std::fs::remove_dir_all(&temp);
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const ROOT: &str = "titan-agent-0.6.1-linux-x86_64";

    fn tarball(count: usize, link: Option<(&str, &str)>) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        for binary in &BINARIES[..count] {
            let mut header = tar::Header::new_gnu();
            header.set_size(1);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, format!("{ROOT}/bin/{binary}"), b"x".as_slice())
                .unwrap();
        }
        if let Some((path, target)) = link {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(EntryType::Symlink);
            header.set_size(0);
            tar.append_link(&mut header, path, target).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    fn extract_into(name: &str, bytes: &[u8]) -> (PathBuf, Result<(), UpdateError>) {
        let root =
            std::env::temp_dir().join(format!("titan-prepare-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("versions")).unwrap();
        let archive = root.join("a.tar.gz");
        std::fs::write(&archive, bytes).unwrap();
        let result = extract(&archive, &root, Version::parse("0.6.1").unwrap());
        (root, result)
    }

    #[test]
    fn a_complete_archive_lands_in_its_version_folder_with_its_modes() {
        let (root, result) = extract_into("ok", &tarball(4, None));
        result.unwrap();
        let daemon = root.join("versions/0.6.1/bin/titan-agentd");
        let mode = std::fs::metadata(&daemon).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
        assert!(is_prepared(&root, Version::parse("0.6.1").unwrap()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_escaping_link_or_a_missing_binary_leaves_nothing() {
        let link = format!("{ROOT}/share/evil");
        let outside = ["..", "..", "..", "outside"].join("/");
        let (root, result) = extract_into("link", &tarball(4, Some((&link, &outside))));
        assert_eq!(result.unwrap_err().code(), "unsafe_entry");
        assert!(!root.join("versions/0.6.1").exists());
        let (other, result) = extract_into("missing", &tarball(3, None));
        assert_eq!(result.unwrap_err().code(), "incomplete");
        assert_eq!(
            std::fs::read_dir(other.join("versions")).unwrap().count(),
            0
        );
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(other);
    }
}
