use crate::{source_db, storage_runtime::ReadOnlySourceStorageHandle};
use persistence_fjall::{
    scan_recovery_consistency, BatchReceiptState, RecoveryScan, RecoveryState, SourceBuildLayout,
};
use persistence_sqlite::repositories::storage_control_repo::StorageControlRepo;
use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageRecoveryError {
    #[error("storage recovery database operation failed: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("storage recovery operation failed: {0}")]
    Storage(#[from] persistence_fjall::FjallStorageError),
    #[error("storage recovery source failed: {0}")]
    Source(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryDisposition {
    Ready,
    RecoveryRequired,
    RepairRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRecoveryItem {
    pub operation_id: String,
    pub source_id: Option<String>,
    pub disposition: RecoveryDisposition,
    pub state: Option<RecoveryState>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRecoveryReport {
    pub disposition: RecoveryDisposition,
    pub items: Vec<StorageRecoveryItem>,
}

pub fn scan_case(
    connection: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<StorageRecoveryReport, StorageRecoveryError> {
    let operations = StorageControlRepo::new(connection).list_open_operations(case_id)?;
    let mut items = Vec::with_capacity(operations.len());
    for operation in operations {
        let Some(source_id) = operation.source_id.clone() else {
            items.push(StorageRecoveryItem {
                operation_id: operation.operation_id,
                source_id: None,
                disposition: RecoveryDisposition::RepairRequired,
                state: None,
                detail: "open storage operation has no source identity".to_owned(),
            });
            continue;
        };
        let Some(handle) = open_source(case_root, &source_id) else {
            items.push(StorageRecoveryItem {
                operation_id: operation.operation_id,
                source_id: Some(source_id),
                disposition: RecoveryDisposition::RepairRequired,
                state: None,
                detail: "source Fjall directory is unavailable".to_owned(),
            });
            continue;
        };
        let layout =
            SourceBuildLayout::for_case_source(case_root, &source_id, &operation.attempt_id)?;
        let receipt = receipt_state(&handle, operation.expected_batch_digest.as_deref())?;
        let report = scan_recovery_consistency(RecoveryScan {
            build_path: &layout.build_path(),
            published_path: &layout.published_path(),
            batch_receipt: receipt,
        });
        let item = match report {
            Ok(report) => {
                let disposition = match report.state {
                    RecoveryState::Published => RecoveryDisposition::Ready,
                    RecoveryState::Sealed => RecoveryDisposition::RecoveryRequired,
                    RecoveryState::MissingBatchReceipt
                    | RecoveryState::MissingManifest
                    | RecoveryState::MissingSeal => RecoveryDisposition::RecoveryRequired,
                    RecoveryState::CorruptBatchReceipt
                    | RecoveryState::ManifestMismatch
                    | RecoveryState::PublishConflict => RecoveryDisposition::RepairRequired,
                };
                StorageRecoveryItem {
                    operation_id: operation.operation_id,
                    source_id: Some(source_id),
                    disposition,
                    state: Some(report.state),
                    detail: format!("storage operation state {:?}", operation.state),
                }
            }
            Err(error) => StorageRecoveryItem {
                operation_id: operation.operation_id,
                source_id: Some(source_id),
                disposition: RecoveryDisposition::RepairRequired,
                state: None,
                detail: error.to_string(),
            },
        };
        items.push(item);
    }
    let disposition = items
        .iter()
        .fold(RecoveryDisposition::Ready, |current, item| {
            match (current, item.disposition) {
                (_, RecoveryDisposition::RepairRequired)
                | (RecoveryDisposition::RepairRequired, _) => RecoveryDisposition::RepairRequired,
                (_, RecoveryDisposition::RecoveryRequired)
                | (RecoveryDisposition::RecoveryRequired, _) => {
                    RecoveryDisposition::RecoveryRequired
                }
                _ => RecoveryDisposition::Ready,
            }
        });
    Ok(StorageRecoveryReport { disposition, items })
}

fn open_source(case_root: &Path, source_id: &str) -> Option<ReadOnlySourceStorageHandle> {
    source_db::open_fjall_source_read_only(case_root, source_id).ok()
}

fn receipt_state(
    handle: &ReadOnlySourceStorageHandle,
    digest: Option<&str>,
) -> Result<BatchReceiptState, StorageRecoveryError> {
    let Some(digest) = digest else {
        return Ok(BatchReceiptState::Missing);
    };
    let bytes =
        hex::decode(digest).map_err(|error| StorageRecoveryError::Source(error.to_string()))?;
    let digest: [u8; 32] = bytes.try_into().map_err(|_| {
        StorageRecoveryError::Source("batch digest must contain 32 bytes".to_owned())
    })?;
    handle
        .batch_receipt_state(&digest)
        .map_err(|error| StorageRecoveryError::Source(error.to_string()))
}
