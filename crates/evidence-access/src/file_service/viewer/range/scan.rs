use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use domain::CaseId;
use memchr::memmem;

use crate::{
    bitlocker_runtime::BitLockerUnlockRegistry,
    file_service::{
        open_preview_session_for_case_with_bitlocker, open_source_for_file_id,
        read_file_bytes_for_case, FileServiceError, PreviewRuntimeRegistry, SourceReadContext,
    },
};

pub const HEX_SCAN_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexScanProgress {
    pub scanned_bytes: u64,
    pub total_bytes: u64,
    pub matches: Vec<u64>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexScanOutcome {
    pub matches: Vec<u64>,
    pub scanned_bytes: u64,
    pub total_bytes: u64,
    pub truncated: bool,
    pub cancelled: bool,
}

#[derive(Clone, Copy)]
pub struct HexScanSessionRequest<'a> {
    pub bitlocker_runtime: &'a Arc<BitLockerUnlockRegistry>,
    pub registry: &'a PreviewRuntimeRegistry,
    pub case_conn: &'a rusqlite::Connection,
    pub case_root: &'a Path,
    pub case_id: &'a CaseId,
    pub file_id: &'a str,
}

/// Scan one independently opened preview session for every match of `pattern`.
///
/// The session is owned by the scan: it is opened from `file_id`, leased for the
/// duration, and closed on every exit path. The caller's preview handles are
/// unrelated and may be closed while the scan runs.
pub fn scan_preview_session_bytes(
    request: HexScanSessionRequest<'_>,
    pattern: &[u8],
    max_matches: usize,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(HexScanProgress),
) -> Result<HexScanOutcome, FileServiceError> {
    let HexScanSessionRequest {
        bitlocker_runtime,
        registry,
        case_conn,
        case_root,
        case_id,
        file_id,
    } = request;
    if pattern.is_empty() {
        return Err(FileServiceError::invalid_input(
            "Search pattern must not be empty",
        ));
    }
    let handle = open_preview_session_for_case_with_bitlocker(
        bitlocker_runtime,
        registry,
        case_conn,
        case_root,
        case_id,
        file_id,
    )?;
    let _close_guard = SessionCloseGuard {
        registry,
        case_id: case_id.0.as_str(),
        handle_id: handle.handle_id.clone(),
    };
    let session = registry.get_session(case_id.0.as_str(), &handle.handle_id)?;
    let total_bytes = session.size();

    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let mut fallback = SourceReadContext::new(
        &source_conn,
        case_conn,
        case_root,
        case_id,
        &global_id.data_source_id,
    )
    .with_bitlocker_runtime(Arc::clone(bitlocker_runtime));

    let mut read_chunk = |offset: u64, length: usize| -> Result<Vec<u8>, FileServiceError> {
        match session.read_prepared_range(offset, length) {
            Ok(Some(bytes)) => Ok(bytes),
            Ok(None) => {
                read_file_bytes_for_case(&mut fallback, &global_id.local_id, offset, length as u32)
            }
            Err(error) if prepared_reader_can_fallback(&error) => {
                tracing::warn!(
                    error = %error,
                    "Prepared hex scan reader failed; retrying through the source-routed reader"
                );
                read_file_bytes_for_case(&mut fallback, &global_id.local_id, offset, length as u32)
            }
            Err(error) => Err(error),
        }
    };
    scan_chunks(
        total_bytes,
        &mut read_chunk,
        pattern,
        max_matches,
        cancel,
        on_progress,
    )
}

fn prepared_reader_can_fallback(error: &FileServiceError) -> bool {
    matches!(error, FileServiceError::Io(_) | FileServiceError::Other(_))
}

struct SessionCloseGuard<'a> {
    registry: &'a PreviewRuntimeRegistry,
    case_id: &'a str,
    handle_id: String,
}

impl Drop for SessionCloseGuard<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.registry.close_session(self.case_id, &self.handle_id) {
            tracing::warn!(error = %error, "Failed to close hex search preview session");
        }
    }
}

/// Streaming matcher over sequential chunks.
///
/// Matches are reported as absolute start offsets in ascending order, with
/// overlapping hits preserved (`AA` in `AAA` yields two offsets). A short tail
/// carried between chunks catches matches that straddle a chunk boundary
/// without copying whole chunks.
pub fn scan_chunks<F>(
    total_bytes: u64,
    mut read_chunk: F,
    pattern: &[u8],
    max_matches: usize,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(HexScanProgress),
) -> Result<HexScanOutcome, FileServiceError>
where
    F: FnMut(u64, usize) -> Result<Vec<u8>, FileServiceError>,
{
    if pattern.is_empty() {
        return Err(FileServiceError::invalid_input(
            "Search pattern must not be empty",
        ));
    }
    let max_matches = max_matches.max(1);
    let overlap = pattern.len() - 1;
    let finder = (pattern.len() > 1).then(|| memmem::Finder::new(pattern));
    let mut matches = Vec::new();
    let mut tail: Vec<u8> = Vec::with_capacity(overlap);
    let mut scanned_bytes = 0u64;
    let mut offset = 0u64;
    let mut truncated = false;
    let mut cancelled = false;

    while offset < total_bytes {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let requested = (total_bytes - offset).min(HEX_SCAN_CHUNK_BYTES as u64) as usize;
        let chunk = read_chunk(offset, requested)?;
        if chunk.is_empty() {
            break;
        }

        if pattern.len() == 1 {
            for position in memchr::memchr_iter(pattern[0], &chunk) {
                matches.push(offset + position as u64);
                if matches.len() >= max_matches {
                    truncated = true;
                    break;
                }
            }
        } else if let Some(finder) = finder.as_ref() {
            if !tail.is_empty() {
                let prefix_len = chunk.len().min(overlap);
                let mut boundary = Vec::with_capacity(tail.len() + prefix_len);
                boundary.extend_from_slice(&tail);
                boundary.extend_from_slice(&chunk[..prefix_len]);
                let mut search_start = 0usize;
                while let Some(position) = finder.find(&boundary[search_start..]) {
                    let start = search_start + position;
                    if start >= tail.len() {
                        break;
                    }
                    matches.push(offset - tail.len() as u64 + start as u64);
                    if matches.len() >= max_matches {
                        truncated = true;
                        break;
                    }
                    search_start = start + 1;
                }
            }
            if !truncated {
                let mut search_start = 0usize;
                while let Some(position) = finder.find(&chunk[search_start..]) {
                    let start = search_start + position;
                    matches.push(offset + start as u64);
                    if matches.len() >= max_matches {
                        truncated = true;
                        break;
                    }
                    search_start = start + 1;
                }
            }
        }

        if overlap > 0 {
            tail.clear();
            tail.extend_from_slice(&chunk[chunk.len().saturating_sub(overlap)..]);
        }
        scanned_bytes = scanned_bytes.saturating_add(chunk.len() as u64);
        offset = offset.saturating_add(chunk.len() as u64);
        on_progress(HexScanProgress {
            scanned_bytes,
            total_bytes,
            matches: matches.clone(),
            truncated,
        });
        if truncated {
            break;
        }
    }

    Ok(HexScanOutcome {
        matches,
        scanned_bytes,
        total_bytes,
        truncated,
        cancelled,
    })
}
