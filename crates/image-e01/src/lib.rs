//! Read-only E01/EWF image access with multi-segment support.

mod open;
mod open_limits;
mod reader;
mod segment;
mod table;

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
