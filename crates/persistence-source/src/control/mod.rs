//! Source control-plane storage (source.db): path layout, identity, path
//! safety, and opening.

pub mod identity;
pub mod opening;
pub mod path_safety;
pub mod paths;

pub use identity::{encode_source_scoped_id, parse_source_scoped_id, GlobalFileId};
pub use opening::{
    checkpoint_source_db, open_registered_source_db, open_registered_source_db_read_only,
    open_source_db, registered_source_db_path, registered_source_index_dir,
    verify_source_db_integrity,
};
pub use path_safety::{
    safe_case_managed_destination, safe_case_relative_path, safe_existing_case_path,
};
pub use paths::{
    canonical_source_db_rel_path, source_content_index_dir, source_db_path, source_dir,
    source_index_dir, source_staging_dir, SourceDbLocator,
};
