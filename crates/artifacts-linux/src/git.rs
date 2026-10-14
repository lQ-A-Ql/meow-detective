//! Bounded Git/GitLab metadata entry points; format-specific parsers live separately.

mod commit;
mod config;
pub(crate) mod model;
mod reflog;
mod refs;

use crate::LinuxArtifactError;
pub use model::GitRecord;
use model::{error, object_id, record, MAX_TEXT_BYTES};
use std::io::Read;

pub fn parse_git_text(path: &str, text: &str) -> Vec<GitRecord> {
    parse_git_text_checked(path, text).unwrap_or_default()
}

pub fn parse_git_bytes(path: &str, bytes: &[u8]) -> Vec<GitRecord> {
    parse_git_bytes_checked(path, bytes).unwrap_or_default()
}

pub fn parse_git_text_checked(
    path: &str,
    text: &str,
) -> Result<Vec<GitRecord>, LinuxArtifactError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(error("Git metadata exceeds 1 MiB"));
    }
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    if normalized.contains("/etc/gitlab/") || normalized.contains("/var/log/gitlab/") {
        return crate::gitlab::parse(&normalized, text);
    }
    if normalized.ends_with("/.git/config") || normalized.ends_with("/.gitmodules") {
        return config::parse(text);
    }
    if normalized.ends_with("/.git/head") {
        let value = text.trim();
        let mut item = record("GitHead", 1, 0);
        item.fields.insert("head".into(), value.into());
        if let Some(reference) = value.strip_prefix("ref: ") {
            item.fields.insert("ref".into(), reference.into());
        } else if object_id(value) {
            item.fields
                .insert("commitHash".into(), value.to_ascii_lowercase());
        } else {
            return Err(error("invalid HEAD"));
        }
        return Ok(vec![item]);
    }
    if normalized.contains("/.git/refs/") || normalized.ends_with("/.git/packed-refs") {
        return refs::parse(path, text);
    }
    if normalized.contains("/.git/logs/") {
        return reflog::parse(text);
    }
    commit::parse(text)
}

pub fn parse_git_bytes_checked(
    path: &str,
    bytes: &[u8],
) -> Result<Vec<GitRecord>, LinuxArtifactError> {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    if normalized.contains("/.git/objects/") {
        let mut payload = Vec::new();
        flate2::read::ZlibDecoder::new(bytes)
            .take(MAX_TEXT_BYTES as u64 + 1)
            .read_to_end(&mut payload)?;
        if payload.len() > MAX_TEXT_BYTES {
            return Err(error("loose commit exceeds 1 MiB"));
        }
        let separator = payload
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| error("loose object has no header terminator"))?;
        let header = std::str::from_utf8(&payload[..separator])
            .map_err(|_| error("invalid loose object header"))?;
        let length = header
            .strip_prefix("commit ")
            .ok_or_else(|| LinuxArtifactError::Unsupported {
                parser: "git.object",
                message: "only loose commit objects are metadata records".into(),
            })?
            .parse::<usize>()
            .map_err(|_| error("invalid loose object size"))?;
        let body = &payload[separator + 1..];
        if body.len() != length {
            return Err(error("loose commit declared size differs from body"));
        }
        return parse_git_text_checked(
            path,
            std::str::from_utf8(body).map_err(|_| error("commit is not UTF-8"))?,
        );
    }
    let mut decoded = Vec::new();
    let bytes = if normalized.ends_with(".gz") {
        flate2::read::MultiGzDecoder::new(bytes)
            .take(MAX_TEXT_BYTES as u64 + 1)
            .read_to_end(&mut decoded)?;
        decoded.as_slice()
    } else {
        bytes
    };
    parse_git_text_checked(
        path,
        std::str::from_utf8(bytes).map_err(|_| error("Git metadata is not UTF-8"))?,
    )
}

#[cfg(test)]
#[path = "../tests/unit/git.rs"]
mod tests;
