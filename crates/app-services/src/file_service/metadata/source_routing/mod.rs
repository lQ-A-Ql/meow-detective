//! Source-routing entry points that resolve case-scoped requests to the
//! per-data-source databases within a case.

mod browse;
mod preview;
mod read;
mod shared;

pub use browse::{
    get_data_sources_for_case, get_file_children_for_case, get_file_jump_context_for_case,
    get_file_rows_for_case, get_file_tree_for_case,
};
pub use preview::{
    document_preview_for_source_case, image_preview_for_source_case,
    media_preview_plan_for_source_case, media_range_for_source_case,
    read_preview_bytes_for_source_case, text_preview_for_source_case,
};
pub use read::{open_file_handle_for_case, read_file_range_for_source_case};
pub(crate) use shared::open_source_for_file_id;

#[cfg(test)]
#[path = "../../../../tests/unit/file_service/source_routing.rs"]
mod tests;
