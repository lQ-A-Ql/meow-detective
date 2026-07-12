//! Read-only E01/EWF image access with multi-segment support.

mod open;
mod reader;
mod segment;
mod table;

pub use reader::E01Reader;

pub(crate) use segment::build_segment_path;
pub(crate) use table::{build_chunk_table, find_geometry, should_read_section_content};

pub(crate) const SECTION_DESCRIPTOR_SIZE: u64 = 76;

#[cfg(test)]
#[path = "../tests/unit/image_e01.rs"]
mod tests;
