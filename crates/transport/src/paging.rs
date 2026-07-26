use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Maximum accepted encoded cursor size. Cursors only contain logical source
/// identifiers and bounded pagination state, never storage paths.
pub const MAX_OPAQUE_CURSOR_LENGTH: usize = 16 * 1024;

const CURSOR_FORMAT_VERSION: &str = "v1";
const CURSOR_INTEGRITY_DOMAIN: &[u8] = b"Meow_Detective:page-cursor:v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorCodecError {
    Empty,
    Oversized,
    InvalidFormat,
    InvalidEncoding,
    IntegrityMismatch,
    InvalidPayload,
}

impl fmt::Display for CursorCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "cursor is empty",
            Self::Oversized => "cursor exceeds the maximum encoded length",
            Self::InvalidFormat => "cursor format or version is invalid",
            Self::InvalidEncoding => "cursor encoding is invalid",
            Self::IntegrityMismatch => "cursor integrity check failed",
            Self::InvalidPayload => "cursor payload is invalid",
        })
    }
}

impl std::error::Error for CursorCodecError {}

pub fn validate_opaque_cursor(cursor: &str) -> Result<(), CursorCodecError> {
    if cursor.is_empty() {
        return Err(CursorCodecError::Empty);
    }
    if cursor.len() > MAX_OPAQUE_CURSOR_LENGTH {
        return Err(CursorCodecError::Oversized);
    }
    if cursor.trim() != cursor || !cursor.is_ascii() {
        return Err(CursorCodecError::InvalidFormat);
    }
    Ok(())
}

pub fn encode_opaque_cursor<T: Serialize>(payload: &T) -> Result<String, CursorCodecError> {
    let payload = serde_json::to_vec(payload).map_err(|_| CursorCodecError::InvalidPayload)?;
    let encoded_payload = URL_SAFE_NO_PAD.encode(&payload);
    let digest = cursor_digest(&payload);
    let cursor = format!(
        "{CURSOR_FORMAT_VERSION}.{encoded_payload}.{}",
        URL_SAFE_NO_PAD.encode(digest)
    );
    validate_opaque_cursor(&cursor)?;
    Ok(cursor)
}

pub fn decode_opaque_cursor<T: DeserializeOwned>(cursor: &str) -> Result<T, CursorCodecError> {
    validate_opaque_cursor(cursor)?;
    let mut parts = cursor.split('.');
    let version = parts.next().ok_or(CursorCodecError::InvalidFormat)?;
    let payload = parts.next().ok_or(CursorCodecError::InvalidFormat)?;
    let digest = parts.next().ok_or(CursorCodecError::InvalidFormat)?;
    if version != CURSOR_FORMAT_VERSION || parts.next().is_some() {
        return Err(CursorCodecError::InvalidFormat);
    }
    let payload = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| CursorCodecError::InvalidEncoding)?;
    let supplied_digest = URL_SAFE_NO_PAD
        .decode(digest)
        .map_err(|_| CursorCodecError::InvalidEncoding)?;
    if !constant_time_eq(&supplied_digest, &cursor_digest(&payload)) {
        return Err(CursorCodecError::IntegrityMismatch);
    }
    serde_json::from_slice(&payload).map_err(|_| CursorCodecError::InvalidPayload)
}

fn cursor_digest(payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(CURSOR_INTEGRITY_DOMAIN);
    hasher.update(payload);
    hasher.finalize().into()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

/// Pagination request parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PageRequest {
    /// Number of items to skip.
    pub offset: u64,
    /// Maximum number of items to return.
    pub limit: u32,
}

impl PageRequest {
    /// Maximum allowed page size to prevent memory exhaustion.
    pub const MAX_LIMIT: u32 = 500;

    /// Default page size.
    pub const DEFAULT_LIMIT: u32 = 100;

    /// Clamp the limit to the maximum allowed value.
    pub fn clamp(&mut self) {
        if self.limit == 0 {
            self.limit = Self::DEFAULT_LIMIT;
        }
        self.limit = self.limit.min(Self::MAX_LIMIT);
    }
}

#[cfg(test)]
#[path = "../tests/unit/paging.rs"]
mod tests;

/// Paginated response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResponse<T> {
    /// Total number of items available.
    pub total: u64,
    /// Items for the current page.
    pub items: Vec<T>,
    /// Opaque continuation token for stable cursor pagination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
