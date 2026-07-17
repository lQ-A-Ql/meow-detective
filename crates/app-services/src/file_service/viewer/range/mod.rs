pub(crate) mod api;
mod content;
mod header;

pub use api::{open_file_content_by_id, read_file_bytes_for_case, read_file_range_for_case};
pub(crate) use header::read_file_header_with_context;
pub use header::{read_file_header_by_id, FileHeaderReadCache};

pub(crate) use api::file_id_from_handle;
