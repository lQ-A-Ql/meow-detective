mod build;
mod identity;
mod opening;
mod path_safety;
mod paths;
mod ready;

use std::path::Path;

use domain::DataSourceId;

pub use crate::storage_runtime::{
    open_source_read_only as open_fjall_source_read_only,
    open_source_read_write as open_fjall_source_read_write,
    source_storage_root as fjall_source_storage_root,
    ReadOnlySourceStorageHandle as FjallReadOnlySourceStorageHandle,
    ReadWriteSourceStorageHandle as FjallReadWriteSourceStorageHandle,
    StorageRuntimeError as FjallStorageRuntimeError,
};
pub(crate) use build::{
    discard_source_build_db, finalize_source_build_db, open_fresh_source_build_db,
    preserve_unpublished_source_build_db, publish_source_build_db, verify_finalized_source_db,
};
pub use identity::{encode_source_scoped_id, parse_source_scoped_id, GlobalFileId};
pub(crate) use opening::ready_data_sources;
pub use opening::{
    checkpoint_source_db, open_registered_source_db, open_registered_source_db_read_only,
    open_source_db, registered_source_db_path, registered_source_index_dir,
    verify_source_db_integrity,
};
pub(crate) use path_safety::safe_case_managed_destination;
pub use path_safety::{safe_case_relative_path, safe_existing_case_path};
pub(crate) use paths::canonical_source_db_rel_path;
pub use paths::{
    source_content_index_dir, source_db_path, source_dir, source_index_dir, source_staging_dir,
    SourceDbLocator,
};

pub fn open_registered_fjall_source_read_only(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<FjallReadOnlySourceStorageHandle, FjallStorageRuntimeError> {
    open_fjall_source_read_only(case_root, &data_source_id.0)
}

pub fn open_registered_fjall_source_read_write(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<FjallReadWriteSourceStorageHandle, FjallStorageRuntimeError> {
    open_fjall_source_read_write(case_root, &data_source_id.0)
}
pub(crate) use ready::open_catalog_recovery_source_by_id;
pub use ready::{
    open_ready_source_by_id, open_ready_source_connections,
    open_ready_source_connections_read_only, open_ready_source_read_only_by_id,
    open_reconstruction_source_by_id, resolve_ready_source_platform, ReadySourceConnection,
    ReadySourceError, ReconstructionSourceConnection, SourceConnectionManager,
};

#[cfg(test)]
#[path = "../tests/unit/source_db.rs"]
mod tests;
