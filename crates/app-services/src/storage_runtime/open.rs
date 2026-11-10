use super::{
    artifact_root, source_registry, DatabaseConfig, FjallStorageError, StorageRuntimeError,
};
use artifact_storage::ArtifactStore;
use persistence_fjall::SourceStore;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) struct OpenParts {
    pub(super) source_root: PathBuf,
    pub(super) source_store: Arc<SourceStore>,
    pub(super) artifact_store: Arc<ArtifactStore>,
}

pub(super) fn open_parts(
    source_root: &Path,
    read_only: bool,
    config: DatabaseConfig,
) -> Result<OpenParts, StorageRuntimeError> {
    let started = std::time::Instant::now();
    if read_only {
        if !source_root.exists() {
            return Err(StorageRuntimeError::MissingRoot(source_root.to_path_buf()));
        }
        if !source_root.is_dir() {
            return Err(StorageRuntimeError::InvalidRoot(source_root.to_path_buf()));
        }
        if !source_root.join("version").is_file() {
            return Err(StorageRuntimeError::MissingRoot(source_root.to_path_buf()));
        }
    } else {
        std::fs::create_dir_all(source_root)
            .map_err(|error| StorageRuntimeError::Source(FjallStorageError::Io(error)))?;
    }
    let artifact_root = artifact_root::artifact_objects_root(source_root);
    let artifact_store = Arc::new(if read_only {
        ArtifactStore::from_existing_objects_root(&artifact_root)?
    } else {
        ArtifactStore::from_objects_root(&artifact_root)?
    });
    let source_store = source_registry::open_shared_source_store(source_root, config)?;
    tracing::debug!(
        source_root = %source_root.display(),
        read_only,
        elapsed_ms = started.elapsed().as_millis(),
        "Fjall source storage opened"
    );
    Ok(OpenParts {
        source_root: source_root.to_path_buf(),
        source_store,
        artifact_store,
    })
}
