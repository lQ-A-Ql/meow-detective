use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryRecord, RecoveryRangeRecord,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use transport::dto::DeletedRecoveryExportDto;

use super::access::open_recovery_content_source;
use super::content::require_verified_content_range;
use super::{DeletedRecoveryContext, DeletedRecoveryError};

const EXPORT_BUFFER_BYTES: usize = 1024 * 1024;

pub fn export_deleted_recovery(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    recovery_id: &str,
    destination_path: &Path,
    overwrite: bool,
) -> Result<DeletedRecoveryExportDto, DeletedRecoveryError> {
    export_deleted_recovery_in_context(
        &DeletedRecoveryContext::new(case_conn, case_root, case_id, data_source_id),
        recovery_id,
        destination_path,
        overwrite,
    )
}

pub(super) fn export_deleted_recovery_in_context(
    context: &DeletedRecoveryContext<'_>,
    recovery_id: &str,
    destination_path: &Path,
    overwrite: bool,
) -> Result<DeletedRecoveryExportDto, DeletedRecoveryError> {
    let mut source = open_recovery_content_source(context, recovery_id)?;
    let outcome = export_complete_content(
        &mut source.reader,
        &source.recovery,
        destination_path,
        overwrite,
    )?;
    Ok(DeletedRecoveryExportDto {
        recovery_id: recovery_id.to_string(),
        bytes_written: outcome.bytes_written,
        sha256: outcome.sha256,
    })
}

#[derive(Debug)]
struct ExportOutcome {
    bytes_written: u64,
    sha256: String,
}

fn export_complete_content<R: Read + Seek + ?Sized>(
    reader: &mut R,
    recovery: &DeletedRecoveryRecord,
    destination_path: &Path,
    overwrite: bool,
) -> Result<ExportOutcome, DeletedRecoveryError> {
    let ranges = complete_content_ranges(recovery)?;
    validate_destination(destination_path, overwrite)?;
    let parent = destination_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::create_dir_all(parent)?;
    }
    let temp_parent = parent.unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(temp_parent)?;
    let mut content_hasher = Sha256::new();
    let mut bytes_written = 0u64;
    for range in ranges {
        bytes_written = bytes_written
            .checked_add(copy_verified_range(
                reader,
                temporary.as_file_mut(),
                range,
                &mut content_hasher,
            )?)
            .ok_or_else(|| {
                DeletedRecoveryError::InvalidState("export byte count overflows".to_string())
            })?;
    }
    if bytes_written != recovery.declared_size {
        return Err(DeletedRecoveryError::Integrity(
            "exported bytes do not match the declared recovered file size".to_string(),
        ));
    }
    let sha256 = hex::encode(content_hasher.finalize());
    let expected = recovery.content_sha256.as_deref().ok_or_else(|| {
        DeletedRecoveryError::Integrity(
            "complete recovery has no persisted complete-content digest".to_string(),
        )
    })?;
    if sha256 != expected {
        return Err(DeletedRecoveryError::Integrity(
            "exported content SHA-256 does not match the persisted complete-content digest"
                .to_string(),
        ));
    }
    temporary.as_file_mut().flush()?;
    temporary.as_file_mut().sync_all()?;
    persist_temporary(temporary, destination_path, overwrite)?;
    Ok(ExportOutcome {
        bytes_written,
        sha256,
    })
}

fn complete_content_ranges(
    recovery: &DeletedRecoveryRecord,
) -> Result<Vec<&RecoveryRangeRecord>, DeletedRecoveryError> {
    if recovery.completeness != "complete" {
        return Err(DeletedRecoveryError::ContentUnavailable(
            "only complete recovery candidates can be exported as files; partial candidates must be inspected through verified ranges"
                .to_string(),
        ));
    }
    let mut ranges = recovery
        .ranges
        .iter()
        .filter(|range| range.range_role == "content")
        .collect::<Vec<_>>();
    ranges.sort_by_key(|range| (range.logical_offset, range.ordinal));
    let mut expected_offset = 0u64;
    for range in &ranges {
        require_verified_content_range(range)?;
        if range.logical_offset != expected_offset {
            return Err(DeletedRecoveryError::Integrity(
                "complete recovery content ranges are not contiguous".to_string(),
            ));
        }
        expected_offset = expected_offset.checked_add(range.length).ok_or_else(|| {
            DeletedRecoveryError::Integrity("complete recovery coverage overflows".to_string())
        })?;
    }
    if expected_offset != recovery.declared_size {
        return Err(DeletedRecoveryError::Integrity(
            "complete recovery ranges do not cover the declared file size".to_string(),
        ));
    }
    Ok(ranges)
}

fn copy_verified_range<R: Read + Seek + ?Sized, W: Write>(
    reader: &mut R,
    output: &mut W,
    range: &RecoveryRangeRecord,
    content_hasher: &mut Sha256,
) -> Result<u64, DeletedRecoveryError> {
    let expected = require_verified_content_range(range)?;
    reader.seek(SeekFrom::Start(range.source_offset))?;
    let mut range_hasher = Sha256::new();
    let mut remaining = range.length;
    let mut buffer = vec![0u8; EXPORT_BUFFER_BYTES];
    while remaining > 0 {
        let length = usize::try_from(remaining.min(EXPORT_BUFFER_BYTES as u64)).map_err(|_| {
            DeletedRecoveryError::InvalidState("export chunk length overflows".to_string())
        })?;
        reader.read_exact(&mut buffer[..length])?;
        output.write_all(&buffer[..length])?;
        range_hasher.update(&buffer[..length]);
        content_hasher.update(&buffer[..length]);
        remaining -= length as u64;
    }
    if hex::encode(range_hasher.finalize()) != expected {
        return Err(DeletedRecoveryError::Integrity(format!(
            "content range {} SHA-256 no longer matches the persisted digest",
            range.ordinal
        )));
    }
    Ok(range.length)
}

fn validate_destination(
    destination_path: &Path,
    overwrite: bool,
) -> Result<(), DeletedRecoveryError> {
    if destination_path.as_os_str().is_empty() {
        return Err(DeletedRecoveryError::InvalidRange(
            "destination path is required".to_string(),
        ));
    }
    if destination_path.is_dir() {
        return Err(DeletedRecoveryError::InvalidRange(
            "destination path must point to a file".to_string(),
        ));
    }
    if destination_path.exists() && !overwrite {
        return Err(DeletedRecoveryError::InvalidRange(
            "destination already exists and overwrite is false".to_string(),
        ));
    }
    Ok(())
}

fn persist_temporary(
    temporary: tempfile::NamedTempFile,
    destination_path: &Path,
    overwrite: bool,
) -> Result<(), DeletedRecoveryError> {
    let result = if overwrite {
        temporary.persist(destination_path)
    } else {
        temporary.persist_noclobber(destination_path)
    };
    result
        .map(|_| ())
        .map_err(|error| DeletedRecoveryError::Io(error.error))
}

#[cfg(test)]
#[path = "../../tests/unit/deleted_recovery/export.rs"]
mod tests;
