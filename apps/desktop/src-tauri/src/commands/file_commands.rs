//! File browsing, preview, media, and extraction commands.

mod browse;
mod extract;
mod media;
mod support;
mod viewer;

pub use browse::{
    get_file_children, get_file_children_request, get_file_jump_context, get_file_rows,
    get_file_rows_request, get_file_tree, get_file_tree_request,
};
pub use extract::extract_file;
pub use media::{get_media_url, read_media_range};
pub use viewer::{
    get_image_preview, get_text_preview, open_file_handle, open_file_handle_request,
    read_file_range,
};

#[cfg(test)]
#[path = "../../tests/unit/commands/file_commands/mod.rs"]
mod tests;
