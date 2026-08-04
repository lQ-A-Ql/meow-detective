//! Read-only Android sparse image access.
//!
//! This crate only maps the Android sparse container to a bounded logical
//! reader. Dynamic partitions and Android filesystem parsing are separate
//! capabilities and must not be folded into this format layer.

mod error;
mod format;
mod reader;

pub use error::{Result, SparseImageError};
pub use format::{
    SparseChecksum, SparseChunk, SparseChunkKind, SparseHeader, SparseImage, SPARSE_CRC32_CHUNK,
    SPARSE_DONT_CARE_CHUNK, SPARSE_FILL_CHUNK, SPARSE_MAGIC, SPARSE_RAW_CHUNK,
};
pub use reader::AndroidSparseReader;
