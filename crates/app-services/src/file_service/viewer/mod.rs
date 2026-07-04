//! Viewer / preview subsystem: file handles, descriptor cache, and range reads.
//!
//! This module was split from a single 3000+ line file into focused submodules:
//!
//! - `descriptor`: preview descriptor construction and caching
//! - `e01_cache`: per-case LRU cache of parsed E01 readers
//! - `handle`: file handle opening and logical-directory path resolution
//! - `image_open`: opening full file content from image-backed sources
//! - `partition`: partition candidate discovery for E01/RAW images
//! - `path`: safe path validation and image path candidate helpers
//! - `range`: public range-read and content-open API
//! - `range_fs`: filesystem-specific fast paths for range reads

mod descriptor;
mod e01_cache;
mod handle;
mod image_open;
mod partition;
mod path;
mod range;
mod range_fs;

pub use e01_cache::{clear_e01_reader_cache, clear_e01_reader_cache_for_case};
pub use handle::{get_file_path_for_entry, open_file_handle_real};
pub use path::safe_relative_path;
pub use range::{
    open_file_content_by_id, read_file_bytes_for_case, read_file_header_by_id,
    read_file_range_for_case, read_file_range_real, FileHeaderReadCache,
};

// Re-exports used by tests and sibling modules.
pub(crate) use descriptor::descriptor_for_file_with_cache;
pub(crate) use e01_cache::open_e01_reader_cached;
#[cfg(test)]
pub(crate) use e01_cache::{E01_READER_CACHE, E01_READER_CACHE_PER_CASE_MAX_SIZE};
pub(crate) use image_open::{open_descriptor_image_file, open_e01_file, open_raw_file};
pub(crate) use partition::{e01_partition_candidates, raw_partition_candidates};
pub(crate) use path::{
    descriptor_file_entry, descriptor_image_path_candidates, entry_image_path_candidates,
};
#[cfg(test)]
pub(crate) use range::read_file_bytes_for_descriptor;
pub(crate) use range_fs::{
    try_read_exfat_image_range_for_descriptor, try_read_exfat_image_range_for_entry,
    try_read_fat_image_range_for_descriptor, try_read_fat_image_range_for_entry,
    try_read_linux_image_range_for_descriptor, try_read_linux_image_range_for_entry,
    try_read_ntfs_image_range_for_descriptor, try_read_ntfs_image_range_for_entry,
};

use crate::file_service::FileServiceError;
use persistence_sqlite::repositories::file_repo::FileRepo;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use transport::dto::ViewerRangeResponseDto;

pub(crate) const FILE_HANDLE_PREFIX: &str = "file:";

/// Internal reader wrapper: seekable filesystems use the fast seek path,
/// streaming-only readers fall back to sequential skip/read.
pub(crate) enum RangeContentReader {
    Seekable(Box<dyn evidence_core::ReadSeek>),
    Streaming(Box<dyn Read>),
}

/// Internal preview locator used by command/runtime-cache layers.
///
/// This intentionally contains only small metadata needed to re-open evidence
/// content; it never stores preview bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDescriptor {
    pub case_id: String,
    pub file_id: String,
    pub source_kind: String,
    pub source_path: String,
    pub partition_index: Option<usize>,
    pub filesystem_kind: Option<String>,
    pub path: String,
    pub mime: Option<String>,
    pub size: u64,
    pub data_source_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partition_candidates: Vec<PreviewPartitionCandidate>,
    /// File entry size at the time the descriptor was built. Used to detect
    /// stale cache entries when the underlying entry is updated in place.
    #[serde(default)]
    pub entry_size: u64,
    /// File entry modified timestamp at the time the descriptor was built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_modified_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPartitionCandidate {
    pub partition_index: usize,
    pub filesystem_kind: String,
    pub offset: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lvm_identity: Option<PreviewLvmIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLvmIdentity {
    pub vg_uuid: String,
    pub vg_name: String,
    pub lv_uuid: String,
    pub lv_name: String,
    pub pv_offsets: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pv_sources: Vec<PreviewLvmPhysicalVolumeSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLvmPhysicalVolumeSource {
    pub source_path: String,
    pub offset: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pv_uuid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pv_name: Option<String>,
}

pub trait PreviewReadContext {
    fn conn(&self) -> &rusqlite::Connection;

    fn case_id(&self) -> &str {
        ""
    }

    fn get_cached_preview_descriptor(&mut self, _key: &str) -> Option<serde_json::Value> {
        None
    }

    fn set_cached_preview_descriptor(&mut self, _key: &str, _value: &serde_json::Value) {}
}

impl PreviewReadContext for &rusqlite::Connection {
    fn conn(&self) -> &rusqlite::Connection {
        self
    }
}

impl<'a, G, S> PreviewReadContext for (&'a rusqlite::Connection, &'a str, G, S)
where
    G: FnMut(&str) -> Option<serde_json::Value>,
    S: FnMut(&str, &serde_json::Value),
{
    fn conn(&self) -> &rusqlite::Connection {
        self.0
    }

    fn case_id(&self) -> &str {
        self.1
    }

    fn get_cached_preview_descriptor(&mut self, key: &str) -> Option<serde_json::Value> {
        (self.2)(key)
    }

    fn set_cached_preview_descriptor(&mut self, key: &str, value: &serde_json::Value) {
        (self.3)(key, value);
    }
}

#[cfg(test)]
pub(crate) static SKIP_READER_BYTES_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
thread_local! {
    pub(crate) static OPEN_FILE_CONTENT_BY_ID_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(crate) static READ_FILE_BYTES_FOR_CASE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) static PREVIEW_DESCRIPTOR_FOR_CASE_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_preview_descriptor_for_case_call_count() {
    PREVIEW_DESCRIPTOR_FOR_CASE_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn preview_descriptor_for_case_call_count() -> usize {
    PREVIEW_DESCRIPTOR_FOR_CASE_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Skip `remaining` bytes on a sequential reader.
///
/// This is exposed because some callers (and tests) need to observe how often
/// a sequential skip is performed versus a seekable read.
pub fn skip_reader_bytes(
    reader: &mut dyn Read,
    mut remaining: u64,
) -> Result<(), FileServiceError> {
    #[cfg(test)]
    SKIP_READER_BYTES_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    if remaining == 0 {
        return Ok(());
    }

    // Sequential skipping is expensive for deep offsets. Large offsets indicate
    // a filesystem reader that should have been opened via the seekable path;
    // log it so regressions can be detected.
    if remaining > 1024 * 1024 {
        tracing::warn!(
            bytes_to_skip = remaining,
            "Sequential byte skip for large offset; consider using a seekable reader"
        );
    }

    // Use a 1 MiB buffer to amortize syscall overhead for the unavoidable
    // sequential-skip path.
    let mut buffer = vec![0u8; 1024 * 1024];
    while remaining > 0 {
        let chunk_len = remaining.min(buffer.len() as u64) as usize;
        let read = reader.read(&mut buffer[..chunk_len])?;
        if read == 0 {
            return Err(FileServiceError::other("Read offset exceeds file size"));
        }
        remaining -= read as u64;
    }
    Ok(())
}

pub(crate) fn read_seekable_range(
    reader: &mut dyn evidence_core::ReadSeek,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, FileServiceError> {
    reader.seek(SeekFrom::Start(offset))?;
    read_bounded(reader, length)
}

pub(crate) fn read_bounded(
    reader: &mut dyn Read,
    length: usize,
) -> Result<Vec<u8>, FileServiceError> {
    let mut bytes = Vec::with_capacity(length);
    reader.take(length as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn empty_hex_response() -> ViewerRangeResponseDto {
    ViewerRangeResponseDto {
        raw_bytes: None,
        kind: "hex".into(),
        lines: Vec::new(),
        encoding: None,
    }
}

pub fn mft_partition_index_from_entry_id(entry_id: &str) -> Option<usize> {
    let mut parts = entry_id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("mft"), Some(partition), Some(_record), None) => partition.parse().ok(),
        _ => None,
    }
}

pub(crate) fn root_partition_index_for_entry(
    repo: &FileRepo<'_>,
    entry: &domain::FileEntry,
) -> Option<usize> {
    if let Some(index) = mft_partition_index_from_entry_id(&entry.id.0) {
        return Some(index);
    }

    let mut current = entry.clone();
    while let Some(parent_id) = &current.parent_id {
        let parent = repo.find_by_id(parent_id).ok()??;
        current = parent;
    }

    current
        .name
        .strip_prefix("Partition ")?
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

/// Build a user-facing error message when all filesystem-specific range fast
/// paths fail and the generic open path also fails. Limits total length and
/// avoids leaking overly long path strings to the UI.
pub(crate) fn format_image_range_error(
    path: &str,
    reasons: &[String],
    fallback_error: Option<&str>,
) -> String {
    const MAX_REASONS: usize = 8;
    const MAX_REASON_LEN: usize = 120;
    const MAX_PATH_LEN: usize = 80;

    let display_path = if path.len() > MAX_PATH_LEN {
        format!("{}...", &path[..MAX_PATH_LEN])
    } else {
        path.to_string()
    };

    let mut summary = reasons
        .iter()
        .take(MAX_REASONS)
        .map(|reason| {
            if reason.len() > MAX_REASON_LEN {
                format!("{}...", &reason[..MAX_REASON_LEN])
            } else {
                reason.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("; ");

    if reasons.len() > MAX_REASONS {
        summary.push_str(&format!("; and {} more", reasons.len() - MAX_REASONS));
    }

    match fallback_error {
        Some(fallback) => format!(
            "Cannot open image-backed file '{}' from any partition. Attempts: {}. Fallback error: {}",
            display_path, summary, fallback
        ),
        None => format!(
            "Cannot open image-backed file '{}' from any partition. Attempts: {}",
            display_path, summary
        ),
    }
}

pub(crate) fn is_fat_filesystem_kind(kind: &str) -> bool {
    matches!(kind, "FAT" | "FAT32" | "FAT16" | "FAT12")
}

pub(crate) fn is_exfat_filesystem_kind(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("exfat") || kind.to_ascii_uppercase().contains("EXFAT")
}

pub(crate) fn is_linux_filesystem_kind(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("ext4")
        || kind.eq_ignore_ascii_case("xfs")
        || kind.eq_ignore_ascii_case("btrfs")
}

pub(crate) fn is_preview_image_filesystem_kind(kind: &str) -> bool {
    kind == "NTFS"
        || is_fat_filesystem_kind(kind)
        || is_exfat_filesystem_kind(kind)
        || is_linux_filesystem_kind(kind)
}

pub(crate) fn looks_like_exfat_boot_sector<R>(reader: &mut R, offset: u64) -> std::io::Result<bool>
where
    R: Read + Seek + ?Sized,
{
    let mut sector = [0u8; 512];
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(&mut sector)?;

    Ok(&sector[3..11] == b"EXFAT   " && sector[510] == 0x55 && sector[511] == 0xAA)
}

/// Try to open the first available path candidate as a non-seekable reader.
pub(crate) fn open_first_image_path(
    fs: &dyn evidence_core::FileSystemReader,
    path_candidates: &[String],
) -> std::io::Result<Box<dyn Read>> {
    let mut last_error = None;
    for path in path_candidates {
        match fs.open_file(path) {
            Ok(reader) => return Ok(reader),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "No preview path candidates")
    }))
}

/// Try to open the first available path candidate as a seekable reader.
/// Falls back to a non-seekable streaming reader if the filesystem does not
/// support seekable file access.
pub(crate) fn open_first_image_path_seekable(
    fs: &dyn evidence_core::FileSystemReader,
    path_candidates: &[String],
) -> std::io::Result<RangeContentReader> {
    let mut last_error = None;

    for path in path_candidates {
        match fs.open_file_seekable(path) {
            Ok(reader) => return Ok(RangeContentReader::Seekable(reader)),
            Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {
                // Filesystem does not provide seekable files; try non-seekable below.
            }
            Err(error) => last_error = Some(error),
        }
    }

    for path in path_candidates {
        match fs.open_file(path) {
            Ok(reader) => return Ok(RangeContentReader::Streaming(reader)),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "No preview path candidates")
    }))
}

#[cfg(test)]
mod tests;
