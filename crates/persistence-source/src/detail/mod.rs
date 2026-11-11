//! Source detail storage (source.fjall).
//!
//! This module tree is the owner of the source detail layer: `source_meta`
//! generation layout, on-disk Fjall stores, record mapping, error text, and
//! persist modes. `persistence-sqlite` no longer ships a copy; source-scoped
//! repositories in [`crate::repositories`] build on this module.

mod mapping;
mod open;
mod read;
mod write;

pub use mapping::{artifact_record, file_entry, file_record, timeline_record};
pub use open::{
    bind_generation, configure_source_fjall, configure_source_identity, has_source_store,
    open_store, open_store_for_write, require_source_store, source_id,
};
pub use read::partition_index;
pub use write::{
    assign_partition_index_to_subtree, set_partition_index, update_catalog_parent,
    update_catalog_path, upsert_artifacts, upsert_files, upsert_files_with_partition,
    upsert_timeline,
};
