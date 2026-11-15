pub(crate) mod api;
mod archive;
mod content;
mod header;
mod logical;
mod scan;

pub use api::{open_file_content_by_id, read_file_bytes_for_case, read_file_range_for_case};
pub use header::{read_file_header_by_id, FileHeaderReadCache};
pub(crate) use header::{
    read_file_header_for_descriptor_with_context, read_file_header_with_context,
};
pub use scan::{
    scan_chunks, scan_preview_session_bytes, HexScanOutcome, HexScanProgress,
    HexScanSessionRequest, HEX_SCAN_CHUNK_BYTES,
};

pub(crate) use api::{file_id_from_handle, read_file_bytes_for_descriptor_with_context};
pub(crate) use content::open_range_content_for_descriptor_with_context;
