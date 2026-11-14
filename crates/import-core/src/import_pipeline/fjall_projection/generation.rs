use persistence_source::fjall::{DatabaseConfig, PersistMode, SourceBuildLayout, SourceStore};
use persistence_sqlite::repositories::storage_control_repo::{
    StorageControlRepo, StorageOperationState,
};
use std::path::{Path, PathBuf};
use transport::CommandError;

pub(super) fn active_generation_root(layout: &SourceBuildLayout) -> (bool, PathBuf) {
    let published = layout.published_path().is_dir();
    let root = if published {
        layout.published_path()
    } else {
        layout.build_path()
    };
    (published, root)
}

pub(super) fn publication_receipt(
    connection: &rusqlite::Connection,
    source_id: &str,
) -> Result<String, CommandError> {
    let source = persistence_source::detail::require_source_store(connection)
        .map_err(CommandError::from_service_error)?;
    let manifest = persistence_source::fjall::read_source_generation_manifest(&source, source_id)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let mut digest = [0u8; 32];
    hex::decode_to_slice(&manifest.projection_batch_digest, &mut digest)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let receipt = source
        .database()
        .batch_receipt_state_for_digest(&digest)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if !matches!(
        receipt,
        persistence_source::fjall::BatchReceiptState::Present { .. }
    ) {
        return Err(CommandError::internal(
            "published source generation batch receipt is missing or corrupt",
        ));
    }
    Ok(manifest.projection_batch_digest)
}

pub(super) fn publish(
    layout: &SourceBuildLayout,
    root: &Path,
    existing: bool,
    source_id: &str,
    attempt: &str,
    digest: &str,
) -> Result<(), CommandError> {
    let expected = if existing {
        layout.published_path()
    } else {
        layout.build_path()
    };
    if expected != root {
        return Err(CommandError::internal(
            "source publication path does not match generation state",
        ));
    }
    let _guard = source_kernel::storage_runtime::close_source_store_at_path(root)
        .map_err(|error| CommandError::internal(error.to_string()))?
        .ok_or_else(|| CommandError::internal("Fjall source generation is still in use"))?;
    let source = SourceStore::open(root, DatabaseConfig::default())
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if existing {
        persistence_source::fjall::replace_source_generation(&source, source_id, attempt, digest)
    } else {
        persistence_source::fjall::seal_source_generation_with_control(
            &source,
            source_id,
            attempt,
            digest,
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| {},
        )
    }
    .map_err(|error| CommandError::internal(error.to_string()))?;
    source
        .database()
        .persist(PersistMode::SyncAll)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    drop(source);
    if !existing {
        persistence_source::fjall::publish_source_generation(root, layout.published_path())
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}

pub(super) fn active_root(
    case_root: &Path,
    case: &str,
    source: &str,
) -> Result<Option<PathBuf>, CommandError> {
    let conn = source_kernel::connection::open_existing_case_db(&case_root.join("app.db"))
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let mut operations = StorageControlRepo::new(&conn)
        .list_open_operations(case)
        .map_err(|error| CommandError::internal(error.to_string()))?
        .into_iter()
        .filter(|operation| {
            operation.source_id.as_deref() == Some(source)
                && operation.operation_kind == "source_import"
                && matches!(
                    operation.state,
                    StorageOperationState::Planned | StorageOperationState::Running
                )
        });
    let Some(operation) = operations.next() else {
        return Ok(None);
    };
    if operations.next().is_some() {
        return Err(CommandError::internal(
            "multiple source import attempts are active",
        ));
    }
    let layout = SourceBuildLayout::for_case_source(case_root, source, &operation.attempt_id)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(Some(active_generation_root(&layout).1))
}
