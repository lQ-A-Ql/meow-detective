use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use base64::Engine;
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryRecord, RecoveryRangeRecord,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use transport::dto::DeletedRecoveryContentRangeDto;

use super::access::open_recovery_content_source;
use super::DeletedRecoveryError;

pub const MAX_RECOVERY_READ_LENGTH: u32 = 1024 * 1024;
const VERIFY_BUFFER_BYTES: usize = 256 * 1024;
const MAX_VERIFICATION_BYTES_PER_READ: u64 = 64 * 1024 * 1024;

pub fn read_deleted_recovery_range(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    recovery_id: &str,
    offset: u64,
    length: u32,
) -> Result<DeletedRecoveryContentRangeDto, DeletedRecoveryError> {
    if length == 0 || length > MAX_RECOVERY_READ_LENGTH {
        return Err(DeletedRecoveryError::InvalidRange(format!(
            "length must be between 1 and {MAX_RECOVERY_READ_LENGTH} bytes"
        )));
    }
    let mut source =
        open_recovery_content_source(case_conn, case_root, case_id, data_source_id, recovery_id)?;
    let read = read_verified_content(&mut source.reader, &source.recovery, offset, length)?;
    Ok(DeletedRecoveryContentRangeDto {
        recovery_id: recovery_id.to_string(),
        offset,
        bytes_base64: base64::engine::general_purpose::STANDARD.encode(&read.bytes),
        bytes_read: u32::try_from(read.bytes.len()).map_err(|_| {
            DeletedRecoveryError::InvalidState("recovery read length exceeds u32".to_string())
        })?,
        declared_size: source.recovery.declared_size,
        eof: read.end == source.recovery.declared_size,
        verified_range_ordinals: read.verified_range_ordinals,
    })
}

#[derive(Debug)]
pub(super) struct VerifiedContentRead {
    pub bytes: Vec<u8>,
    pub end: u64,
    pub verified_range_ordinals: Vec<u32>,
}

pub(super) fn read_verified_content<R: Read + Seek + ?Sized>(
    reader: &mut R,
    recovery: &DeletedRecoveryRecord,
    offset: u64,
    length: u32,
) -> Result<VerifiedContentRead, DeletedRecoveryError> {
    let end = requested_end(recovery, offset, length)?;
    let ranges = covering_ranges(recovery, offset, end)?;
    let verification_bytes = ranges.iter().try_fold(0u64, |total, range| {
        total.checked_add(range.length).ok_or_else(|| {
            DeletedRecoveryError::InvalidState("verification byte count overflows".to_string())
        })
    })?;
    if verification_bytes > MAX_VERIFICATION_BYTES_PER_READ {
        return Err(DeletedRecoveryError::ContentUnavailable(format!(
            "a bounded read would require verifying more than {MAX_VERIFICATION_BYTES_PER_READ} source bytes"
        )));
    }

    let capacity = usize::try_from(end - offset).map_err(|_| {
        DeletedRecoveryError::InvalidRange("requested range exceeds platform limits".to_string())
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    let mut verified_range_ordinals = Vec::with_capacity(ranges.len());
    for range in ranges {
        verify_and_collect_range(reader, range, offset, end, &mut bytes)?;
        verified_range_ordinals.push(range.ordinal);
    }
    if bytes.len() != capacity {
        return Err(DeletedRecoveryError::Integrity(
            "verified source ranges did not produce the requested logical bytes".to_string(),
        ));
    }
    Ok(VerifiedContentRead {
        bytes,
        end,
        verified_range_ordinals,
    })
}

fn requested_end(
    recovery: &DeletedRecoveryRecord,
    offset: u64,
    length: u32,
) -> Result<u64, DeletedRecoveryError> {
    if recovery.completeness == "metadata_only" {
        return Err(DeletedRecoveryError::ContentUnavailable(
            "the candidate contains verified metadata only".to_string(),
        ));
    }
    if length == 0 || length > MAX_RECOVERY_READ_LENGTH {
        return Err(DeletedRecoveryError::InvalidRange(format!(
            "length must be between 1 and {MAX_RECOVERY_READ_LENGTH} bytes"
        )));
    }
    if offset >= recovery.declared_size {
        return Err(DeletedRecoveryError::InvalidRange(
            "offset is outside the recovered file".to_string(),
        ));
    }
    offset
        .checked_add(u64::from(length))
        .map(|end| end.min(recovery.declared_size))
        .ok_or_else(|| DeletedRecoveryError::InvalidRange("range end overflows".to_string()))
}

fn covering_ranges(
    recovery: &DeletedRecoveryRecord,
    offset: u64,
    end: u64,
) -> Result<Vec<&RecoveryRangeRecord>, DeletedRecoveryError> {
    let mut ranges = recovery
        .ranges
        .iter()
        .filter(|range| range.range_role == "content")
        .collect::<Vec<_>>();
    ranges.sort_by_key(|range| (range.logical_offset, range.ordinal));

    let mut covered_until = offset;
    let mut selected = Vec::new();
    for range in ranges {
        let range_end = range
            .logical_offset
            .checked_add(range.length)
            .ok_or_else(|| {
                DeletedRecoveryError::Integrity("stored logical range overflows".to_string())
            })?;
        if range_end <= covered_until || range.logical_offset >= end {
            continue;
        }
        if range.logical_offset > covered_until {
            break;
        }
        require_verified_content_range(range)?;
        selected.push(range);
        covered_until = range_end;
        if covered_until >= end {
            break;
        }
    }
    if covered_until < end {
        return Err(DeletedRecoveryError::ContentUnavailable(
            "the requested logical range crosses an unrecovered gap".to_string(),
        ));
    }
    Ok(selected)
}

pub(super) fn require_verified_content_range(
    range: &RecoveryRangeRecord,
) -> Result<&str, DeletedRecoveryError> {
    if range.range_role != "content"
        || range.source_kind != "filesystem"
        || range.allocation_state != "free"
    {
        return Err(DeletedRecoveryError::Integrity(
            "stored recovery content range is not allocation-verified filesystem data".to_string(),
        ));
    }
    range.sha256.as_deref().ok_or_else(|| {
        DeletedRecoveryError::Integrity(
            "stored recovery content range has no SHA-256 digest".to_string(),
        )
    })
}

fn verify_and_collect_range<R: Read + Seek + ?Sized>(
    reader: &mut R,
    range: &RecoveryRangeRecord,
    request_start: u64,
    request_end: u64,
    output: &mut Vec<u8>,
) -> Result<(), DeletedRecoveryError> {
    let expected_hash = require_verified_content_range(range)?;
    reader.seek(SeekFrom::Start(range.source_offset))?;
    let mut remaining = range.length;
    let mut range_position = 0u64;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; VERIFY_BUFFER_BYTES];
    while remaining > 0 {
        let chunk_len =
            usize::try_from(remaining.min(VERIFY_BUFFER_BYTES as u64)).map_err(|_| {
                DeletedRecoveryError::InvalidState("chunk length overflows".to_string())
            })?;
        reader.read_exact(&mut buffer[..chunk_len])?;
        let chunk = &buffer[..chunk_len];
        hasher.update(chunk);
        collect_overlap(
            range,
            range_position,
            chunk,
            request_start,
            request_end,
            output,
        )?;
        remaining -= chunk_len as u64;
        range_position += chunk_len as u64;
    }
    let actual_hash = hex::encode(hasher.finalize());
    if actual_hash != expected_hash {
        return Err(DeletedRecoveryError::Integrity(format!(
            "content range {} SHA-256 no longer matches the persisted digest",
            range.ordinal
        )));
    }
    Ok(())
}

fn collect_overlap(
    range: &RecoveryRangeRecord,
    range_position: u64,
    chunk: &[u8],
    request_start: u64,
    request_end: u64,
    output: &mut Vec<u8>,
) -> Result<(), DeletedRecoveryError> {
    let chunk_start = range
        .logical_offset
        .checked_add(range_position)
        .ok_or_else(|| DeletedRecoveryError::Integrity("chunk offset overflows".to_string()))?;
    let chunk_end = chunk_start
        .checked_add(chunk.len() as u64)
        .ok_or_else(|| DeletedRecoveryError::Integrity("chunk end overflows".to_string()))?;
    let overlap_start = chunk_start.max(request_start);
    let overlap_end = chunk_end.min(request_end);
    if overlap_start >= overlap_end {
        return Ok(());
    }
    let start = usize::try_from(overlap_start - chunk_start).map_err(|_| {
        DeletedRecoveryError::InvalidState("overlap start exceeds usize".to_string())
    })?;
    let end = usize::try_from(overlap_end - chunk_start)
        .map_err(|_| DeletedRecoveryError::InvalidState("overlap end exceeds usize".to_string()))?;
    output.extend_from_slice(&chunk[start..end]);
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/deleted_recovery/content.rs"]
mod tests;
