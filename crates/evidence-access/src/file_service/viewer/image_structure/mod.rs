//! Shared bounded image container analysis for viewer metadata and batch extraction.
//!
//! The analyzer only walks bytes; callers supply an [`ImageByteSource`]. It never
//! receives host paths and never writes evidence.

mod analyze;
mod bmp;
mod exif;
mod gif;
mod ico;
mod jpeg;
mod limits;
mod map;
mod png;
mod postprocess;
mod report_helpers;
mod source;
mod tiff;
mod types;
mod webp;

pub use analyze::{analyze_image, ImageAnalyzeOptions, ImageByteSource, ImageStructureError};
pub use limits::{ANALYSIS_SCAN_LIMIT, BUFFER_FALLBACK_LIMIT};
pub use map::to_dto;
pub use source::SeekableSource;
pub use types::*;

use report_helpers::{
    mark_truncated, push_finding, push_segment, push_warning, read_u16, read_u32, set_declared_end,
};

#[cfg(test)]
#[path = "../../../../tests/unit/file_service/image_structure/mod.rs"]
mod tests;
