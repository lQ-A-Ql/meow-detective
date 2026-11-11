//! Source storage runtime: shared Fjall source stores, artifact store handles,
//! and source-scoped projection helpers.

mod artifact_root;
mod extensions;
mod open;
pub mod partition_marker;
mod registry;
pub mod search_marker;
mod source_counts;
mod storage;

use std::path::{Path, PathBuf};

pub use registry::{
    close_case_source_stores, close_source_store, close_source_store_at_path, SourceStoreCloseGuard,
};
pub use storage::{
    CaseStorageHandle, ReadOnly, ReadOnlyCaseStorageHandle, ReadOnlySourceStorageHandle,
    ReadOnlyStorageRuntime, ReadWrite, ReadWriteCaseStorageHandle, ReadWriteSourceStorageHandle,
    ReadWriteStorageRuntime, SourceStorageHandle, StorageRuntime, StorageRuntimeError,
};

pub fn source_storage_root(
    case_root: impl AsRef<Path>,
    source_id: &str,
) -> Result<PathBuf, StorageRuntimeError> {
    registry::source_storage_root(case_root.as_ref(), source_id)
}

pub fn open_source_read_only(
    case_root: impl AsRef<Path>,
    source_id: &str,
) -> Result<ReadOnlySourceStorageHandle, StorageRuntimeError> {
    let source_root = source_storage_root(case_root, source_id)?;
    Ok(ReadOnlyStorageRuntime::open(source_root)?.source())
}

pub fn open_source_read_write(
    case_root: impl AsRef<Path>,
    source_id: &str,
) -> Result<ReadWriteSourceStorageHandle, StorageRuntimeError> {
    let source_root = source_storage_root(case_root, source_id)?;
    Ok(ReadWriteStorageRuntime::open(source_root)?.source())
}

pub fn open_source_read_write_at_path(
    source_root: impl AsRef<Path>,
) -> Result<ReadWriteSourceStorageHandle, StorageRuntimeError> {
    Ok(ReadWriteStorageRuntime::open(source_root)?.source())
}
