//! HTTP of the updater: one `GET` of the manifest and its signature and,
//! only when there is a newer version, the download of its archive. No
//! authentication, cookies, identifiers nor version in the `User-Agent`
//! (ADR-0011 §6).

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};
use titan_agent_core::update::{
    Artifact, MAX_ARTIFACT_BYTES, UpdateError, check_download, hex, manifest_url, signature_url,
};

/// Largest manifest and signature read.
const MANIFEST_LIMIT: u64 = 1024 * 1024;
const SIGNATURE_LIMIT: u64 = 16 * 1024;

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent("titan-agent")
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(30 * 60)))
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>, UpdateError> {
    agent
        .get(url)
        .call()
        .map_err(|error| UpdateError::Download(format!("{url}: {error}")))?
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|error| UpdateError::Download(format!("{url}: {error}")))
}

/// The manifest bytes and the text of its signature.
pub fn manifest(agent: &ureq::Agent, base_url: &str) -> Result<(Vec<u8>, String), UpdateError> {
    let manifest = get(agent, &manifest_url(base_url), MANIFEST_LIMIT)?;
    let signature = get(agent, &signature_url(base_url), SIGNATURE_LIMIT)?;
    let signature = String::from_utf8(signature).map_err(|_| UpdateError::Signature)?;
    Ok((manifest, signature))
}

/// Downloads `url` to `path`, hashing on the way and never past the
/// artifact's size, then checks size and SHA-256.
pub fn download(
    agent: &ureq::Agent,
    url: &str,
    artifact: &Artifact,
    path: &Path,
) -> Result<(), UpdateError> {
    let io = |error: std::io::Error| UpdateError::Io(format!("{}: {error}", path.display()));
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| UpdateError::Download(format!("{url}: {error}")))?;
    let limit = artifact.size.min(MAX_ARTIFACT_BYTES) + 1;
    let mut body = response
        .body_mut()
        .with_config()
        .limit(limit)
        .reader()
        .take(limit);
    let mut file = std::fs::File::create(path).map_err(io)?;
    let mut hasher = Sha256::new();
    let mut size: u64 = 0;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = match body.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            // Past the limit the reader fails: it is too long.
            Err(_) if size >= artifact.size => {
                size += 1;
                break;
            }
            Err(error) => return Err(UpdateError::Download(format!("{url}: {error}"))),
        };
        size += read as u64;
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(io)?;
    }
    file.sync_all().map_err(io)?;
    check_download(size, &hex(&hasher.finalize()), artifact)
}
