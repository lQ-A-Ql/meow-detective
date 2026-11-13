use super::{RecoveryDisposition, StorageRecoveryError, StorageRecoveryItem};
use crate::{source_db, storage_runtime::ReadOnlySourceStorageHandle};
use persistence_source::fjall::{
    scan_recovery_consistency, BatchReceiptState, DatabaseConfig, RecoveryScan, RecoveryState,
    SourceBuildLayout, SourceStore, SOURCE_GENERATION_MANIFEST_FILE,
};
use persistence_sqlite::repositories::storage_control_repo::{
    StorageControlRepo, StorageOperationRecord, StorageOperationState,
};
use rusqlite::Connection;
use std::path::Path;

mod readiness;

struct OperationStorage {
    layout: SourceBuildLayout,
    handle: Option<ReadOnlySourceStorageHandle>,
    build_exists: bool,
    receipt: BatchReceiptState,
    recovered_digest: Option<String>,
}

pub(super) fn scan_open_operations(
    connection: &Connection,
    case_root: &Path,
    operations: Vec<StorageOperationRecord>,
) -> Result<Vec<StorageRecoveryItem>, StorageRecoveryError> {
    operations
        .into_iter()
        .map(|operation| scan_operation(connection, case_root, operation))
        .collect()
}

fn scan_operation(
    connection: &Connection,
    case_root: &Path,
    operation: StorageOperationRecord,
) -> Result<StorageRecoveryItem, StorageRecoveryError> {
    let Some(source_id) = operation.source_id.clone() else {
        return Ok(StorageRecoveryItem {
            operation_id: operation.operation_id,
            source_id: None,
            disposition: RecoveryDisposition::RepairRequired,
            state: None,
            detail: "open storage operation has no source identity".to_owned(),
        });
    };
    let Some(mut storage) = operation_storage(connection, case_root, &operation, &source_id)?
    else {
        return Ok(StorageRecoveryItem {
            operation_id: operation.operation_id,
            source_id: Some(source_id),
            disposition: RecoveryDisposition::RepairRequired,
            state: None,
            detail: "source Fjall build and published directories are unavailable".to_owned(),
        });
    };
    let mut report = scan_consistency(&storage);
    publish_sealed_generation(case_root, &operation, &source_id, &mut storage, &mut report)?;
    let generation_validation = if report
        .as_ref()
        .is_ok_and(|report| report.state == RecoveryState::Published)
    {
        validate_published_generation(&storage, &operation, &source_id)
            .map_err(|error| error.to_string())
    } else {
        Ok(())
    };
    let mut item = recovery_item(report, &operation, &source_id);
    if let Err(error) = generation_validation {
        item.disposition = RecoveryDisposition::RepairRequired;
        item.detail = format!("source generation verification failed: {error}");
        item.state = Some(RecoveryState::ManifestMismatch);
    }
    if operation.operation_kind == "source_import"
        && item.disposition == RecoveryDisposition::Ready
        && operation.state != StorageOperationState::Published
    {
        readiness::finish_import_recovery(
            connection,
            case_root,
            &source_id,
            &operation.operation_id,
        )?;
    }
    Ok(item)
}

fn operation_storage(
    connection: &Connection,
    case_root: &Path,
    operation: &StorageOperationRecord,
    source_id: &str,
) -> Result<Option<OperationStorage>, StorageRecoveryError> {
    let layout = SourceBuildLayout::for_case_source(case_root, source_id, &operation.attempt_id)?;
    let handle = open_source(case_root, source_id);
    let build_exists = layout.build_path().is_dir();
    let build_store = if handle.is_none()
        && build_exists
        && (operation.expected_batch_digest.is_some()
            || layout
                .build_path()
                .join(SOURCE_GENERATION_MANIFEST_FILE)
                .is_file())
    {
        Some(SourceStore::open(
            layout.build_path(),
            DatabaseConfig::default(),
        )?)
    } else {
        None
    };
    if handle.is_none() && build_store.is_none() && !build_exists {
        return Ok(None);
    }
    let (receipt, recovered_digest) = operation_receipt(
        operation,
        source_id,
        &layout,
        handle.as_ref(),
        build_store.as_ref(),
    )?;
    drop(build_store);
    if let Some(digest) = recovered_digest.as_deref() {
        let repo = StorageControlRepo::new(connection);
        repo.update_expected_batch_digest(&operation.operation_id, digest)?;
        if operation.state != StorageOperationState::Committed {
            repo.update_operation_state(
                &operation.operation_id,
                StorageOperationState::Committed,
                &chrono::Utc::now().to_rfc3339(),
                None,
            )?;
        }
    }
    Ok(Some(OperationStorage {
        layout,
        handle,
        build_exists,
        receipt,
        recovered_digest,
    }))
}

fn operation_receipt(
    operation: &StorageOperationRecord,
    source_id: &str,
    layout: &SourceBuildLayout,
    handle: Option<&ReadOnlySourceStorageHandle>,
    build_store: Option<&SourceStore>,
) -> Result<(BatchReceiptState, Option<String>), StorageRecoveryError> {
    match (handle, build_store) {
        (Some(handle), _)
            if operation.operation_kind == "source_import"
                && layout
                    .published_path()
                    .join(SOURCE_GENERATION_MANIFEST_FILE)
                    .is_file() =>
        {
            source_import_receipt_state_from_handle(
                handle,
                source_id,
                operation.expected_batch_digest.as_deref(),
            )
        }
        (Some(handle), _) => Ok((
            receipt_state(handle, operation.expected_batch_digest.as_deref())?,
            None,
        )),
        (None, Some(store))
            if operation.operation_kind == "source_import"
                && layout
                    .build_path()
                    .join(SOURCE_GENERATION_MANIFEST_FILE)
                    .is_file() =>
        {
            source_import_receipt_state_from_store(
                store,
                source_id,
                operation.expected_batch_digest.as_deref(),
            )
        }
        (None, Some(store)) => Ok((
            receipt_state_from_database(store, operation.expected_batch_digest.as_deref())?,
            None,
        )),
        (None, None) => Ok((BatchReceiptState::Missing, None)),
    }
}

fn scan_consistency(
    storage: &OperationStorage,
) -> Result<
    persistence_source::fjall::RecoveryConsistencyReport,
    persistence_source::fjall::FjallStorageError,
> {
    scan_recovery_consistency(RecoveryScan {
        build_path: &storage.layout.build_path(),
        published_path: &storage.layout.published_path(),
        batch_receipt: storage.receipt,
    })
}

fn publish_sealed_generation(
    case_root: &Path,
    operation: &StorageOperationRecord,
    source_id: &str,
    storage: &mut OperationStorage,
    report: &mut Result<
        persistence_source::fjall::RecoveryConsistencyReport,
        persistence_source::fjall::FjallStorageError,
    >,
) -> Result<(), StorageRecoveryError> {
    let source_generation = storage
        .layout
        .build_path()
        .join(SOURCE_GENERATION_MANIFEST_FILE)
        .is_file();
    if operation.operation_kind != "source_import"
        || storage.handle.is_some()
        || !storage.build_exists
        || !source_generation
        || !report
            .as_ref()
            .is_ok_and(|report| report.state == RecoveryState::Sealed)
    {
        return Ok(());
    }
    let build_store =
        match SourceStore::open(storage.layout.build_path(), DatabaseConfig::default()) {
            Ok(store) => store,
            Err(error) => {
                *report = Err(error);
                return Ok(());
            }
        };
    let manifest = match persistence_source::fjall::verify_source_generation_with_control(
        &build_store,
        source_id,
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    ) {
        Ok(manifest) => manifest,
        Err(error) => {
            *report = Err(error);
            return Ok(());
        }
    };
    if storage
        .recovered_digest
        .as_deref()
        .or(operation.expected_batch_digest.as_deref())
        .is_some_and(|digest| digest != manifest.projection_batch_digest)
    {
        *report = Err(
            persistence_source::fjall::FjallStorageError::InvalidEnvelope(
                "SQLite operation digest differs from source generation receipt".to_owned(),
            ),
        );
        return Ok(());
    }
    drop(build_store);
    if let Err(error) = persistence_source::fjall::publish_source_generation(
        storage.layout.build_path(),
        storage.layout.published_path(),
    ) {
        *report = Err(error);
        return Ok(());
    }
    storage.handle = open_source(case_root, source_id);
    let Some(handle) = storage.handle.as_ref() else {
        *report = Err(
            persistence_source::fjall::FjallStorageError::InvalidBuildPath(
                storage.layout.published_path().display().to_string(),
            ),
        );
        return Ok(());
    };
    storage.receipt = receipt_state(
        handle,
        storage
            .recovered_digest
            .as_deref()
            .or(operation.expected_batch_digest.as_deref()),
    )?;
    *report = scan_consistency(storage);
    Ok(())
}

fn validate_published_generation(
    storage: &OperationStorage,
    operation: &StorageOperationRecord,
    source_id: &str,
) -> Result<(), StorageRecoveryError> {
    if !storage.layout.published_path().is_dir()
        || !storage
            .layout
            .published_path()
            .join(SOURCE_GENERATION_MANIFEST_FILE)
            .is_file()
    {
        return Ok(());
    }
    let handle = storage
        .handle
        .as_ref()
        .expect("published Fjall source handle exists");
    validate_published_generation_receipt(
        handle,
        source_id,
        storage
            .recovered_digest
            .as_deref()
            .or(operation.expected_batch_digest.as_deref()),
    )
}

fn validate_published_generation_receipt(
    handle: &ReadOnlySourceStorageHandle,
    source_id: &str,
    expected_digest: Option<&str>,
) -> Result<(), StorageRecoveryError> {
    let manifest = match handle.verify_source_generation_with_control(
        source_id,
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    ) {
        Ok(manifest) => manifest,
        Err(error) => return Err(StorageRecoveryError::Source(error.to_string())),
    };
    if expected_digest.is_some_and(|digest| digest != manifest.projection_batch_digest) {
        return Err(StorageRecoveryError::Source(
            "SQLite operation digest differs from source generation receipt".to_owned(),
        ));
    }
    match handle
        .batch_receipt_state(&decode_batch_digest(&manifest.projection_batch_digest)?)
        .map_err(|error| StorageRecoveryError::Source(error.to_string()))?
    {
        BatchReceiptState::Present { .. } => Ok(()),
        BatchReceiptState::Missing | BatchReceiptState::Corrupt => {
            Err(StorageRecoveryError::Source(
                "source generation batch receipt is missing or corrupt".to_owned(),
            ))
        }
    }
}

fn recovery_item(
    report: Result<
        persistence_source::fjall::RecoveryConsistencyReport,
        persistence_source::fjall::FjallStorageError,
    >,
    operation: &StorageOperationRecord,
    source_id: &str,
) -> StorageRecoveryItem {
    match report {
        Ok(report) => {
            let disposition = match report.state {
                RecoveryState::Published => RecoveryDisposition::Ready,
                RecoveryState::Sealed
                | RecoveryState::MissingBatchReceipt
                | RecoveryState::MissingManifest
                | RecoveryState::MissingSeal => RecoveryDisposition::RecoveryRequired,
                RecoveryState::CorruptBatchReceipt
                | RecoveryState::ManifestMismatch
                | RecoveryState::PublishConflict => RecoveryDisposition::RepairRequired,
            };
            StorageRecoveryItem {
                operation_id: operation.operation_id.clone(),
                source_id: Some(source_id.to_owned()),
                disposition,
                state: Some(report.state),
                detail: format!("storage operation state {:?}", operation.state),
            }
        }
        Err(error) => StorageRecoveryItem {
            operation_id: operation.operation_id.clone(),
            source_id: Some(source_id.to_owned()),
            disposition: RecoveryDisposition::RepairRequired,
            state: None,
            detail: error.to_string(),
        },
    }
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
    handle
        .batch_receipt_state(&decode_batch_digest(digest)?)
        .map_err(|error| StorageRecoveryError::Source(error.to_string()))
}

fn receipt_state_from_database(
    store: &SourceStore,
    digest: Option<&str>,
) -> Result<BatchReceiptState, StorageRecoveryError> {
    let Some(digest) = digest else {
        return Ok(BatchReceiptState::Missing);
    };
    store
        .database()
        .batch_receipt_state_for_digest(&decode_batch_digest(digest)?)
        .map_err(StorageRecoveryError::Storage)
}

fn source_import_receipt_state_from_handle(
    handle: &ReadOnlySourceStorageHandle,
    source_id: &str,
    expected_digest: Option<&str>,
) -> Result<(BatchReceiptState, Option<String>), StorageRecoveryError> {
    let manifest = match handle.verify_source_generation_with_control(
        source_id,
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    ) {
        Ok(manifest) => manifest,
        Err(crate::storage_runtime::StorageRuntimeError::Source(error)) => {
            return Ok((invalid_manifest_receipt_state(&error), None));
        }
        Err(_) => return Ok((BatchReceiptState::Corrupt, None)),
    };
    if expected_digest.is_some_and(|digest| digest != manifest.projection_batch_digest) {
        return Ok((BatchReceiptState::Corrupt, None));
    }
    let state = handle
        .batch_receipt_state(&decode_batch_digest(&manifest.projection_batch_digest)?)
        .map_err(|error| StorageRecoveryError::Source(error.to_string()))?;
    let recovered = (expected_digest.is_none()
        && matches!(state, BatchReceiptState::Present { .. }))
    .then_some(manifest.projection_batch_digest);
    Ok((state, recovered))
}

fn source_import_receipt_state_from_store(
    store: &SourceStore,
    source_id: &str,
    expected_digest: Option<&str>,
) -> Result<(BatchReceiptState, Option<String>), StorageRecoveryError> {
    let manifest = match persistence_source::fjall::verify_source_generation_with_control(
        store,
        source_id,
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    ) {
        Ok(manifest) => manifest,
        Err(error) => return Ok((invalid_manifest_receipt_state(&error), None)),
    };
    if expected_digest.is_some_and(|digest| digest != manifest.projection_batch_digest) {
        return Ok((BatchReceiptState::Corrupt, None));
    }
    let state = receipt_state_from_database(store, Some(&manifest.projection_batch_digest))?;
    let recovered = (expected_digest.is_none()
        && matches!(state, BatchReceiptState::Present { .. }))
    .then_some(manifest.projection_batch_digest);
    Ok((state, recovered))
}

fn decode_batch_digest(value: &str) -> Result<[u8; 32], StorageRecoveryError> {
    let bytes =
        hex::decode(value).map_err(|error| StorageRecoveryError::Source(error.to_string()))?;
    bytes
        .try_into()
        .map_err(|_| StorageRecoveryError::Source("batch digest must contain 32 bytes".to_owned()))
}

fn invalid_manifest_receipt_state(
    error: &persistence_source::fjall::FjallStorageError,
) -> BatchReceiptState {
    match error {
        persistence_source::fjall::FjallStorageError::MissingManifest(_)
        | persistence_source::fjall::FjallStorageError::MissingSeal(_) => {
            BatchReceiptState::Missing
        }
        _ => BatchReceiptState::Corrupt,
    }
}
