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
    let mut matcher = ChunkMatcher::new(pattern, max_matches.max(1));
    let mut matches = Vec::new();
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
        truncated = matcher.scan(&chunk, offset, &mut matches);
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

struct ChunkMatcher<'a> {
    pattern: &'a [u8],
    finder: Option<memmem::Finder<'a>>,
    max_matches: usize,
    tail: Vec<u8>,
}

impl<'a> ChunkMatcher<'a> {
    fn new(pattern: &'a [u8], max_matches: usize) -> Self {
        Self {
            pattern,
            finder: (pattern.len() > 1).then(|| memmem::Finder::new(pattern)),
            max_matches,
            tail: Vec::with_capacity(pattern.len().saturating_sub(1)),
        }
    }

    /// Scan one chunk and refresh the boundary tail. Returns `true` when the
    /// match cap was reached.
    fn scan(&mut self, chunk: &[u8], offset: u64, matches: &mut Vec<u64>) -> bool {
        let mut truncated = false;
        if self.pattern.len() == 1 {
            for position in memchr::memchr_iter(self.pattern[0], chunk) {
                matches.push(offset + position as u64);
                if matches.len() >= self.max_matches {
                    truncated = true;
                    break;
                }
            }
        } else {
            self.scan_multi(chunk, offset, matches, &mut truncated);
            let overlap = self.pattern.len() - 1;
            self.tail.clear();
            self.tail
                .extend_from_slice(&chunk[chunk.len().saturating_sub(overlap)..]);
        }
        truncated
    }

    fn scan_multi(&self, chunk: &[u8], offset: u64, matches: &mut Vec<u64>, truncated: &mut bool) {
        let Some(finder) = self.finder.as_ref() else {
            return;
        };
        if !self.tail.is_empty() {
            let prefix_len = chunk.len().min(self.pattern.len() - 1);
            let mut boundary = Vec::with_capacity(self.tail.len() + prefix_len);
            boundary.extend_from_slice(&self.tail);
            boundary.extend_from_slice(&chunk[..prefix_len]);
            let mut search_start = 0usize;
            while let Some(position) = finder.find(&boundary[search_start..]) {
                let start = search_start + position;
                if start >= self.tail.len() {
                    break;
                }
                matches.push(offset - self.tail.len() as u64 + start as u64);
                if matches.len() >= self.max_matches {
                    *truncated = true;
                    break;
                }
                search_start = start + 1;
            }
        }
        if *truncated {
            return;
        }
        let mut search_start = 0usize;
        while let Some(position) = finder.find(&chunk[search_start..]) {
            let start = search_start + position;
            matches.push(offset + start as u64);
            if matches.len() >= self.max_matches {
                *truncated = true;
                break;
            }
            search_start = start + 1;
        }
    }
}
