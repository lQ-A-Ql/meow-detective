use std::path::Path;

use domain::{CaseId, DataSourceId};
use evidence_core::EvidenceReader;
use persistence_sqlite::repositories::deleted_recovery_repo::{
    DeletedRecoveryRecord, DeletedRecoveryRepo, RecoveryScanRecord,
};
use rusqlite::Connection;

use super::source::{load_source, load_targets, open_target_reader};
use super::DeletedRecoveryError;

pub(super) struct RecoveryContentSource {
    pub recovery: DeletedRecoveryRecord,
    pub reader: Box<dyn EvidenceReader>,
}

pub(super) fn open_recovery_content_source(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    recovery_id: &str,
) -> Result<RecoveryContentSource, DeletedRecoveryError> {
    let ready = crate::source_db::open_ready_source_read_only_by_id(
        case_conn,
        case_root,
        case_id,
        data_source_id,
    )?;
    let (scan, recovery) = DeletedRecoveryRepo::new(&ready.connection)
        .find_recovery(&data_source_id.0, recovery_id)?
        .ok_or_else(|| DeletedRecoveryError::RecoveryNotFound {
            data_source_id: data_source_id.0.clone(),
            recovery_id: recovery_id.to_string(),
        })?;
    require_content_candidate(&recovery)?;

    let source = load_source(case_conn, data_source_id)?;
    let target = load_targets(
        &ready.connection,
        data_source_id,
        ready.platform,
        Some(scan.partition_index),
    )?
    .into_iter()
    .find(|target| target.partition.partition_index == scan.partition_index)
    .ok_or_else(|| {
        DeletedRecoveryError::ContentUnavailable(
            "the recovery partition is no longer readable".to_string(),
        )
    })?;
    verify_target_identity(&scan, target.filesystem_type)?;
    let (reader, offset) = open_target_reader(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        &source,
        &target,
    )?;
    let reader = if scan.filesystem_type == "ntfs" {
        let filesystem = fs_ntfs::NtfsReader::open(reader, offset)?;
        verify_ntfs_volume_identity(&scan, &filesystem)?;
        verify_ntfs_candidate(&filesystem, &recovery)?;
        filesystem.into_reader()
    } else {
        reader
    };
    Ok(RecoveryContentSource { recovery, reader })
}

fn verify_ntfs_volume_identity(
    scan: &RecoveryScanRecord,
    filesystem: &fs_ntfs::NtfsReader,
) -> Result<(), DeletedRecoveryError> {
    let current = format!("{:016x}", filesystem.volume_serial());
    if scan.filesystem_uuid.as_deref() != Some(current.as_str()) {
        return Err(DeletedRecoveryError::Integrity(
            "stored NTFS volume serial no longer matches the routed filesystem".to_string(),
        ));
    }
    Ok(())
}

fn verify_ntfs_candidate(
    filesystem: &fs_ntfs::NtfsReader,
    recovery: &DeletedRecoveryRecord,
) -> Result<(), DeletedRecoveryError> {
    let sequence = recovery.mft_sequence.ok_or_else(|| {
        DeletedRecoveryError::Integrity(
            "NTFS recovery candidate has no persisted MFT sequence number".to_string(),
        )
    })?;
    let record_number = recovery.inode.parse::<u64>().map_err(|_| {
        DeletedRecoveryError::Integrity(
            "NTFS recovery candidate inode is not a valid MFT record number".to_string(),
        )
    })?;
    filesystem.validate_deleted_file_record(record_number, sequence)?;
    Ok(())
}

fn require_content_candidate(recovery: &DeletedRecoveryRecord) -> Result<(), DeletedRecoveryError> {
    if recovery.entry_type.as_deref() != Some("file") {
        return Err(DeletedRecoveryError::ContentUnavailable(
            "only regular-file recovery candidates expose content".to_string(),
        ));
    }
    if recovery.completeness == "metadata_only" {
        return Err(DeletedRecoveryError::ContentUnavailable(
            "the candidate contains verified metadata only".to_string(),
        ));
    }
    Ok(())
}

fn verify_target_identity(
    scan: &RecoveryScanRecord,
    current_filesystem_type: &str,
) -> Result<(), DeletedRecoveryError> {
    if scan.filesystem_type != current_filesystem_type {
        return Err(DeletedRecoveryError::Integrity(format!(
            "stored filesystem type '{}' no longer matches routed filesystem type '{}'",
            scan.filesystem_type, current_filesystem_type
        )));
    }
    Ok(())
}
