//! File browsing, preview, media, and extraction commands.

mod bitlocker;
mod browse;
mod extract;
mod media;
mod recovery;
mod support;
mod viewer;

pub use bitlocker::{
    forget_persisted_bitlocker_key, import_unlocked_bitlocker_catalog, inspect_bitlocker_volume,
    lock_bitlocker_volume, restore_persisted_bitlocker_key, unlock_bitlocker_with_password,
    unlock_bitlocker_with_recovery_password,
};
pub use browse::{
    get_file_children, get_file_children_request, get_file_jump_context, get_file_rows,
    get_file_rows_request, get_file_tree, get_file_tree_request,
};
pub use extract::extract_file;
pub use media::{get_media_url, read_media_range};
pub use recovery::{
    export_deleted_recovery, list_deleted_recoveries, read_deleted_recovery_range,
    run_deleted_recovery,
};
pub use viewer::{
    close_file_handle, get_document_preview, get_image_preview, get_text_preview, open_file_handle,
    open_file_handle_request, read_file_range,
};

#[cfg(test)]
#[path = "../../tests/unit/commands/file_commands/mod.rs"]
mod tests;
