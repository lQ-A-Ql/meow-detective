mod build;
mod identity;
mod migration;
mod opening;
mod path_safety;
mod paths;
mod ready;

pub(crate) use build::{
    discard_source_build_db, finalize_source_build_db, open_fresh_source_build_db,
    preserve_unpublished_source_build_db, publish_source_build_db, verify_finalized_source_db,
};
pub use identity::{encode_source_scoped_id, parse_source_scoped_id, GlobalFileId};
pub use migration::migrate_ready_source_databases;
pub use opening::{
    checkpoint_source_db, open_registered_source_db, open_registered_source_db_read_only,
    open_source_db, registered_source_db_path, registered_source_index_dir,
    verify_source_db_integrity,
};
pub(crate) use opening::{open_registered_reconstruction_source_db_read_only, ready_data_sources};
pub use path_safety::{safe_case_relative_path, safe_existing_case_path};
pub(crate) use paths::canonical_source_db_rel_path;
pub use paths::{
    source_content_index_dir, source_db_path, source_dir, source_index_dir, source_staging_dir,
    SourceDbLocator,
};
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
