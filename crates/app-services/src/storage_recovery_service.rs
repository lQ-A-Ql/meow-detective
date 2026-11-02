use crate::{source_db, storage_runtime::ReadOnlySourceStorageHandle};
use artifact_storage::{ArtifactStore, ObjectAddress};
use persistence_fjall::{
    scan_recovery_consistency, BatchReceiptState, RecoveryScan, RecoveryState, SourceBuildLayout,
};
use persistence_sqlite::repositories::storage_control_repo::{
    StorageControlRepo, StorageOperationState,
};
use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

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
    let mut items = Vec::with_capacity(operations.len() + 1);
    verify_manifests(connection, case_root, case_id, &mut items)?;
    verify_artifacts(connection, case_root, case_id, &mut items)?;
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

fn verify_manifests(
    connection: &Connection,
    case_root: &Path,
    case_id: &str,
    items: &mut Vec<StorageRecoveryItem>,
) -> Result<(), StorageRecoveryError> {
    for manifest in StorageControlRepo::new(connection).list_manifests(case_id)? {
        let path = case_root.join(&manifest.relative_path);
        if !path.is_dir() {
            items.push(StorageRecoveryItem {
                operation_id: format!("manifest:{}", manifest.manifest_id),
                source_id: manifest.source_id,
                disposition: RecoveryDisposition::RepairRequired,
                state: None,
                detail: "registered storage manifest path is missing".to_owned(),
            });
        }
    }
    Ok(())
}

pub fn repair_operation(
    connection: &Connection,
    operation_id: &str,
    reason: &str,
) -> Result<(), StorageRecoveryError> {
    let repo = StorageControlRepo::new(connection);
    let operation = repo.find_operation(operation_id)?.ok_or_else(|| {
        StorageRecoveryError::Source("storage operation was not found".to_owned())
    })?;
    repo.update_operation_state(
        operation_id,
        StorageOperationState::Failed,
        &chrono::Utc::now().to_rfc3339(),
        Some(reason),
    )?;
    repo.insert_repair_event(
        &persistence_sqlite::repositories::storage_control_repo::StorageRepairEventRecord {
            event_id: Uuid::new_v4().to_string(),
            case_id: operation.case_id,
            source_id: operation.source_id,
            operation_id: Some(operation_id.to_owned()),
            event_kind: "storage_operation_repair".to_owned(),
            state: "failed".to_owned(),
            details_json: Some(serde_json::json!({"reason": reason}).to_string()),
            error_code: Some(reason.to_owned()),
        },
    )?;
    Ok(())
}

fn verify_artifacts(
    connection: &Connection,
    case_root: &Path,
    case_id: &str,
    items: &mut Vec<StorageRecoveryItem>,
) -> Result<(), StorageRecoveryError> {
    let store = ArtifactStore::from_objects_root(case_root.join("artifacts").join("objects"))
        .map_err(|error| StorageRecoveryError::Source(error.to_string()))?;
    for object in StorageControlRepo::new(connection).list_ready_artifacts(case_id)? {
        let address = ObjectAddress::parse(&object.sha256)
            .map_err(|error| StorageRecoveryError::Source(error.to_string()))?;
        if let Err(error) = store.verify(&address) {
            items.push(StorageRecoveryItem {
                operation_id: format!("artifact:{}", object.object_id),
                source_id: None,
                disposition: RecoveryDisposition::RepairRequired,
                state: None,
                detail: format!("artifact object verification failed: {error}"),
            });
        }
    }
    Ok(())
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
