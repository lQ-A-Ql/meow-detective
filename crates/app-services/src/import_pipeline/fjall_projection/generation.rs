use persistence_source::fjall::{DatabaseConfig, PersistMode, SourceBuildLayout, SourceStore};
use persistence_sqlite::repositories::storage_control_repo::{
    StorageControlRepo, StorageOperationState,
};
use std::path::{Path, PathBuf};
use transport::CommandError;

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
    let _guard = crate::storage_runtime::close_source_store_at_path(root)
        .map_err(|error| CommandError::internal(error.to_string()))?
        .ok_or_else(|| CommandError::internal("Fjall source generation is still in use"))?;
    let source = SourceStore::open(root, DatabaseConfig::default())
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if existing {
        persistence_source::fjall::replace_source_generation(&source, source_id, attempt, digest)
    } else {
        persistence_source::fjall::seal_source_generation(&source, source_id, attempt, digest)
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
    let conn = crate::connection::open_existing_case_db(&case_root.join("app.db"))
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
    Ok(Some(if layout.published_path().is_dir() {
        layout.published_path()
    } else {
        layout.build_path()
    }))
}
