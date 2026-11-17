//! Pure logic of the updater (docs/Decisiones/ADR-0011, §3 and §7): the
//! signed manifest, versions, the checks before installing anything, the
//! contents of the archive, when it is safe to apply and what the launcher
//! does at start. No I/O: the daemon and the launcher do the reading,
//! downloading and writing.
//!
//! The archive checks live in `update/archive.rs`; the install layout,
//! the safe condition and the launcher's decisions in `update/install.rs`.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod archive;
mod install;

pub use archive::{EntryKind, bundle_root, check_bundle, check_entry};
pub use install::{
    APP_ID, INSTALL_DIR, LaunchStep, Pending, RolledBack, apply_blockers, config_dir, data_home,
    install_root, launch_step, safe_to_apply, versions_to_remove,
};

/// Format of the manifest this app understands.
pub const MANIFEST_FORMAT: u32 = 1;
/// Largest artifact the app downloads (256 MiB).
pub const MAX_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;
/// Where the releases live. Every URL is built from it, never read from
/// the manifest.
pub const RELEASES_URL: &str = "https://github.com/danielperezmartinez/titan-agent/releases";
/// File name of the manifest in a release.
pub const MANIFEST_NAME: &str = "titan-agent-update.json";
/// The four binaries of a release, in its `bin/`.
pub const BINARIES: [&str; 4] = [
    "titan-agent",
    "titan-agentd",
    "titan-team",
    "titan-agent-launcher",
];
/// Target of the Linux release.
pub const LINUX_TARGET: &str = "linux-x86_64";
/// The project's current public key, a minisign `.pub` file.
const PROJECT_KEY: &str = include_str!("../../../packaging/minisign.pub");

/// Public keys accepted for the manifest (the base64 line of a minisign
/// `.pub`). Rotation adds the new key here in a release signed with the
/// old one (ADR-0011 §3); `packaging/minisign.pub` is the current one.
pub fn accepted_keys() -> Vec<&'static str> {
    [public_key_line(PROJECT_KEY)]
        .into_iter()
        .filter(|key| !key.is_empty())
        .collect()
}

/// A release version: strict `MAJOR.MINOR.PATCH`, no pre-release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// Parses `0.6.1`: three numbers without signs, spaces, a `v` or
    /// leading zeros.
    pub fn parse(text: &str) -> Option<Version> {
        let mut parts = text.split('.');
        let mut number = || -> Option<u64> {
            let part = parts.next()?;
            let digits = !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
            if !digits || (part.len() > 1 && part.starts_with('0')) {
                return None;
            }
            part.parse().ok()
        };
        let version = Version {
            major: number()?,
            minor: number()?,
            patch: number()?,
        };
        parts.next().is_none().then_some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// `titan-agent-update.json`, format 1. Unknown fields are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub version: String,
    pub published: String,
    pub notes_url: String,
    pub artifacts: Vec<Artifact>,
}

/// One downloadable file of a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub target: String,
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

/// What [`select`] chose to install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    pub version: Version,
    pub artifact: Artifact,
    /// Download URL, built by the app.
    pub url: String,
    pub notes_url: String,
}

/// Why an update is not installed. [`UpdateError::code`] is stable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// No accepted key verifies it, or its trusted comment does not match.
    Signature,
    Manifest(String),
    Format(u32),
    /// The release is not newer than the installed version (no update).
    NotNewer(String),
    BadVersion(String),
    NoArtifact(String),
    BadName(String),
    TooBig(u64),
    SizeMismatch { expected: u64, got: u64 },
    HashMismatch,
    UnsafeEntry(String),
    Incomplete(String),
    /// The network or the server failed.
    Download(String),
    /// A local file could not be read or written.
    Io(String),
}

impl UpdateError {
    pub fn code(&self) -> &'static str {
        match self {
            UpdateError::Signature => "signature",
            UpdateError::Manifest(_) => "manifest",
            UpdateError::Format(_) => "format",
            UpdateError::NotNewer(_) => "not_newer",
            UpdateError::BadVersion(_) => "bad_version",
            UpdateError::NoArtifact(_) => "no_artifact",
            UpdateError::BadName(_) => "bad_name",
            UpdateError::TooBig(_) => "too_big",
            UpdateError::SizeMismatch { .. } => "size_mismatch",
            UpdateError::HashMismatch => "hash_mismatch",
            UpdateError::UnsafeEntry(_) => "unsafe_entry",
            UpdateError::Incomplete(_) => "incomplete",
            UpdateError::Download(_) => "download",
            UpdateError::Io(_) => "io",
        }
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateError::Signature => write!(f, "la firma del manifiesto no es válida"),
            UpdateError::Manifest(why) => write!(f, "manifiesto ilegible: {why}"),
            UpdateError::Format(n) => write!(f, "formato de manifiesto desconocido ({n})"),
            UpdateError::NotNewer(v) => write!(f, "la versión {v} no es más nueva"),
            UpdateError::BadVersion(v) => write!(f, "la versión {v} ya falló al arrancar"),
            UpdateError::NoArtifact(t) => write!(f, "la versión no tiene paquete para {t}"),
            UpdateError::BadName(n) => write!(f, "nombre de paquete no válido: {n:?}"),
            UpdateError::TooBig(n) => write!(f, "el paquete es demasiado grande ({n} bytes)"),
            UpdateError::SizeMismatch { expected, got } => {
                write!(f, "la descarga mide {got} bytes y debía medir {expected}")
            }
            UpdateError::HashMismatch => write!(f, "el SHA-256 de la descarga no coincide"),
            UpdateError::UnsafeEntry(p) => write!(f, "entrada del paquete no permitida: {p}"),
            UpdateError::Incomplete(b) => write!(f, "al paquete le falta bin/{b}"),
            UpdateError::Download(why) => write!(f, "no se pudo descargar: {why}"),
            UpdateError::Io(why) => write!(f, "error de disco: {why}"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// The trusted (signed) comment of the manifest of `version`.
pub fn trusted_comment(version: &str) -> String {
    format!("titan-agent-update format={MANIFEST_FORMAT} version={version}")
}

/// Verifies the signature of the manifest with any of `keys` (base64
/// minisign public keys) and reads it. The trusted comment must name the
/// manifest's own version, so a signed manifest cannot be replayed as
/// another version.
pub fn verify_manifest(
    manifest: &[u8],
    signature: &str,
    keys: &[&str],
) -> Result<Manifest, UpdateError> {
    let signature =
        minisign_verify::Signature::decode(signature).map_err(|_| UpdateError::Signature)?;
    let verified = keys.iter().any(|key| {
        minisign_verify::PublicKey::from_base64(key.trim())
            .is_ok_and(|key| key.verify(manifest, &signature, false).is_ok())
    });
    if !verified {
        return Err(UpdateError::Signature);
    }
    let parsed: Manifest = serde_json::from_slice(manifest)
        .map_err(|error| UpdateError::Manifest(error.to_string()))?;
    if parsed.format != MANIFEST_FORMAT {
        return Err(UpdateError::Format(parsed.format));
    }
    if signature.trusted_comment() != trusted_comment(&parsed.version) {
        return Err(UpdateError::Signature);
    }
    Ok(parsed)
}

/// Decides whether `manifest` is an update for `installed` on `target`
/// (ADR-0011 §3, checks 3 to 5).
pub fn select(
    manifest: &Manifest,
    installed: Version,
    bad: &[Version],
    target: &str,
    base_url: &str,
) -> Result<Selected, UpdateError> {
    let version = Version::parse(&manifest.version).ok_or_else(|| {
        UpdateError::Manifest(format!("versión no válida: {:?}", manifest.version))
    })?;
    if version <= installed {
        return Err(UpdateError::NotNewer(version.to_string()));
    }
    if bad.contains(&version) {
        return Err(UpdateError::BadVersion(version.to_string()));
    }
    let artifact = manifest
        .artifacts
        .iter()
        .find(|a| a.target == target)
        .ok_or_else(|| UpdateError::NoArtifact(target.to_string()))?;
    let name = &artifact.name;
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return Err(UpdateError::BadName(name.clone()));
    }
    if artifact.size > MAX_ARTIFACT_BYTES {
        return Err(UpdateError::TooBig(artifact.size));
    }
    Ok(Selected {
        version,
        artifact: artifact.clone(),
        url: format!("{}/download/v{version}/{name}", base(base_url)),
        notes_url: manifest.notes_url.clone(),
    })
}

fn base(base_url: &str) -> &str {
    base_url.trim_end_matches('/')
}

/// URL of the manifest of the latest release (never a pre-release).
pub fn manifest_url(base_url: &str) -> String {
    format!("{}/latest/download/{MANIFEST_NAME}", base(base_url))
}

/// URL of the manifest's minisign signature.
pub fn signature_url(base_url: &str) -> String {
    format!("{}.minisig", manifest_url(base_url))
}

/// SHA-256 of `bytes` in lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Lowercase hex of a digest.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The downloaded file must have exactly the size and SHA-256 of its
/// artifact. The size is checked first.
pub fn check_download(size: u64, sha256_hex: &str, artifact: &Artifact) -> Result<(), UpdateError> {
    if size != artifact.size {
        return Err(UpdateError::SizeMismatch {
            expected: artifact.size,
            got: size,
        });
    }
    if !sha256_hex.eq_ignore_ascii_case(&artifact.sha256) || sha256_hex.len() != 64 {
        return Err(UpdateError::HashMismatch);
    }
    Ok(())
}

/// Reads the public key line of a minisign `.pub` file (the line after the
/// `untrusted comment:`), or the text itself if it is just the key.
pub fn public_key_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("untrusted comment:"))
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_strictly_and_order_numerically() {
        assert_eq!(
            Version::parse("0.10.2"),
            Some(Version {
                major: 0,
                minor: 10,
                patch: 2
            })
        );
        for bad in ["1.2", "1.2.3.4", "1..2", "+1.2.3", "1.2.x"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert!(Version::parse("1.0.0") > Version::parse("0.99.99"));
    }

    #[test]
    fn the_key_line_skips_the_untrusted_comment() {
        assert_eq!(
            public_key_line("untrusted comment: minisign public key 1\nRWQabc\n"),
            "RWQabc"
        );
        assert_eq!(public_key_line("RWQabc"), "RWQabc");
    }

    #[test]
    fn a_short_hash_never_matches() {
        let artifact = Artifact {
            target: LINUX_TARGET.into(),
            name: "x.tar.gz".into(),
            sha256: String::new(),
            size: 1,
        };
        assert_eq!(check_download(1, "", &artifact), Err(UpdateError::HashMismatch));
    }
}
