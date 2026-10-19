//! Read-only E01/EWF image access with multi-segment support.

mod checksum;
mod geometry;
mod open;
mod open_limits;
mod paged_index;
mod reader;
mod sections;
mod segment;
mod table_validation;

mod open_plan;
pub use open_plan::E01OpenPlan;
pub use reader::E01Reader;

pub(crate) use segment::build_segment_path;
pub(crate) const SECTION_DESCRIPTOR_SIZE: u64 = 76;
pub fn segment_path(path: &std::path::Path, index: u32) -> std::path::PathBuf {
    build_segment_path(path, index)
}

#[cfg(test)]
#[path = "../tests/unit/image_e01.rs"]
mod tests;
