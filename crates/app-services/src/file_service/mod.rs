//! File browsing, metadata routing, preview, and extraction services.

mod browse;
mod checkpointed_enumeration;
mod data_sources;
mod derived_filesystem;
mod enumeration;
mod error;
mod extraction;
pub(crate) mod filesystem_locators;
mod metadata;
mod mft;
mod partition_roots;
mod preview_runtime;
mod source_read;
mod viewer;
mod visibility;

pub use browse::{
    get_file_children_lazy, get_file_children_lazy_with_visibility, get_file_jump_context,
    get_file_rows_for_request, get_file_tree_real, get_file_tree_real_with_visibility,
};
pub(crate) use checkpointed_enumeration::replace_placeholder_root_checkpointed;
pub use data_sources::{
    get_data_sources_real, get_recent_objects_for_case, get_recent_objects_real,
    rename_data_source_real,
};
pub use enumeration::{
    enumerate_filesystem, enumerate_filesystem_with_root_name,
    enumerate_filesystem_with_root_name_and_cancel, EnumerationStats,
};
pub use error::FileServiceError;
pub use extraction::{extract_file_to_destination, extract_file_to_destination_for_case};
pub use metadata::preview_sessions::{
    close_preview_session_for_case, invalidate_preview_source, open_preview_session_for_case,
    preview_session_file_id, preview_session_metadata, read_preview_session_bytes_for_case,
    read_preview_session_media_range_for_case, read_preview_session_range_for_case,
};
pub use metadata::source_routing::{
    document_preview_for_source_case, get_data_sources_for_case, get_file_children_for_case,
    get_file_jump_context_for_case, get_file_rows_for_case, get_file_tree_for_case,
    image_preview_for_source_case, media_preview_plan_for_source_case, media_range_for_source_case,
    open_file_handle_for_case, read_file_range_for_source_case, read_preview_bytes_for_source_case,
    text_preview_for_source_case,
};
pub use mft::{
    add_entry_to_path_map, enumerate_filesystem_mft, mft_parent_entry_id, parse_ntfs_data_runs,
    populate_file_graph_for_data_source, read_ntfs_mft_stream, records_to_file_entries,
    update_entry_parent_ids, update_entry_paths,
};
pub use partition_roots::{
    insert_partition_placeholder_root, remove_partition_placeholder_root,
    replace_placeholder_root_with_real, replace_placeholder_root_with_real_and_cancel,
    store_data_source_partitions,
};
pub use preview_runtime::{PreviewRuntimeRegistry, PreviewRuntimeStats};
pub(crate) use source_read::{PreparedSourceReadState, SourceReadContext, SourceReadFileHint};
pub use viewer::{
    clear_e01_reader_cache, clear_e01_reader_cache_for_case, get_file_path_for_entry,
    image_preview_for_file, media_preview_plan_for_file, media_range_for_file,
    open_file_content_by_id, open_file_handle_real, read_file_bytes_for_case,
    read_file_header_by_id, read_file_range_for_case, read_preview_bytes_for_file,
    safe_relative_path, skip_reader_bytes, text_preview_for_file, FileHeaderReadCache,
    MediaPreviewPlan,
};
pub(crate) use viewer::{
    preview_partition_candidate_from_record, PreviewDescriptor, PreviewPartitionCandidate,
    RangeContentReader,
};

#[cfg(test)]
#[path = "../../tests/unit/file_service/core.rs"]
mod tests;
