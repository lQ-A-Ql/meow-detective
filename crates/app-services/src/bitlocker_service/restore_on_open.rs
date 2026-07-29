use std::path::Path;

use domain::CaseId;
use persistence_sqlite::repositories::bitlocker_restore_intent_repo::{
    BitLockerRestoreIntentRepo, BitLockerRestoreStatus,
};
use rusqlite::Connection;

use super::{
    persistence::restore_persisted_bitlocker_key_for_fingerprint, BitLockerRuntimeContext,
    BitLockerServiceError,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BitLockerRestoreSummary {
    pub attempted: usize,
    pub restored: usize,
    pub failed: usize,
    pub disabled: usize,
}

/// Restores only volumes explicitly enabled after a verified unlock. A failure
/// is isolated to its volume so opening a case never depends on a credential.
pub fn restore_enabled_bitlocker_volumes(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    runtimes: BitLockerRuntimeContext<'_>,
) -> Result<BitLockerRestoreSummary, BitLockerServiceError> {
    let repo = BitLockerRestoreIntentRepo::new(case_conn);
    let intents = repo.list_enabled_for_case(case_id)?;
    let mut summary = BitLockerRestoreSummary::default();
    for intent in intents {
        summary.attempted += 1;
        match restore_persisted_bitlocker_key_for_fingerprint(
            case_conn,
            case_root,
            case_id,
            &intent.data_source_id,
            intent.partition_index,
            Some(&intent.metadata_fingerprint),
            runtimes,
        ) {
            Ok(_) => {
                record_status(
                    &repo,
                    &intent.data_source_id,
                    intent.partition_index,
                    BitLockerRestoreStatus::Restored,
                    None,
                );
                summary.restored += 1;
            }
            Err(error) => {
                let status = restore_failure_status(&error);
                record_status(
                    &repo,
                    &intent.data_source_id,
                    intent.partition_index,
                    status,
                    stable_error_code(&error),
                );
                if status == BitLockerRestoreStatus::Disabled {
                    summary.disabled += 1;
                } else {
                    summary.failed += 1;
                }
                tracing::warn!(
                    data_source_id = intent.data_source_id.0,
                    partition_index = intent.partition_index,
                    error_code = stable_error_code(&error),
                    "BitLocker volume was not restored while opening the case"
                );
            }
        }
    }
    Ok(summary)
}

fn restore_failure_status(error: &BitLockerServiceError) -> BitLockerRestoreStatus {
    match error {
        BitLockerServiceError::StoredKeyNotFound
        | BitLockerServiceError::PersistedKeyFingerprintMismatch
        | BitLockerServiceError::PartitionNotFound { .. }
        | BitLockerServiceError::NotBitLocker { .. }
        | BitLockerServiceError::UnsupportedSourceKind { .. }
        | BitLockerServiceError::UnsupportedFilesystem(_)
        | BitLockerServiceError::Volume(volume_bitlocker::BitLockerError::PersistedKeyInvalid {
            ..
        })
        | BitLockerServiceError::KeyStore(super::BitLockerKeyStoreError::Unsupported)
        | BitLockerServiceError::KeyStore(super::BitLockerKeyStoreError::CorruptBlob(_)) => {
            BitLockerRestoreStatus::Disabled
        }
        _ => BitLockerRestoreStatus::Failed,
    }
}

fn stable_error_code(error: &BitLockerServiceError) -> Option<&'static str> {
    use transport::ServiceErrorCategory;
    error.code().or(Some("BITLOCKER_RESTORE_FAILED"))
}

fn record_status(
    repo: &BitLockerRestoreIntentRepo<'_>,
    data_source_id: &domain::DataSourceId,
    partition_index: u32,
    status: BitLockerRestoreStatus,
    error_code: Option<&str>,
) {
    if let Err(error) = repo.mark_status(data_source_id, partition_index, status, error_code) {
        tracing::warn!(
            data_source_id = data_source_id.0,
            partition_index,
            %error,
            "Failed to record BitLocker restore status"
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/bitlocker_restore_on_open.rs"]
mod tests;
