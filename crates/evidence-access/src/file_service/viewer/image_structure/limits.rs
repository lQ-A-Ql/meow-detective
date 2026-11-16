//! Hard bounds for image container analysis.

use infrastructure::constants::{ARTIFACT_FILE_LIMIT_BYTES, MAX_INLINE_IMAGE_PREVIEW_BYTES};

pub(super) const VIEWER_RANDOM_READ_LIMIT: usize = 2 * 1024 * 1024;
pub(super) const ANALYSIS_RANDOM_READ_LIMIT: usize = 8 * 1024 * 1024;
pub(super) const VIEWER_SCAN_LIMIT: usize = MAX_INLINE_IMAGE_PREVIEW_BYTES as usize;
pub const ANALYSIS_SCAN_LIMIT: usize = 8 * 1024 * 1024;
pub const BUFFER_FALLBACK_LIMIT: usize = 8 * 1024 * 1024;

pub(super) const SEGMENT_LIMIT: usize = 8192;
pub(super) const REPORTED_SEGMENT_LIMIT: usize = 256;
pub(super) const EXIF_BLOCK_LIMIT: usize = 16;
pub(super) const EXIF_BLOCK_READ_LIMIT: usize = 1024 * 1024;
pub(super) const IFD_LIMIT: usize = 32;
pub(super) const IFD_ENTRY_LIMIT: usize = 2048;
pub(super) const ICO_ENTRY_LIMIT: usize = 1024;
pub(super) const GIF_SUB_BLOCK_LIMIT: usize = 16_384;

pub(super) const TRAILING_HASH_LIMIT: usize = 1024 * 1024;
pub(super) const WHOLE_HASH_LIMIT: u64 = ARTIFACT_FILE_LIMIT_BYTES;
