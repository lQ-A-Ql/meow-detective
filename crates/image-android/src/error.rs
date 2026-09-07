use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, SparseImageError>;

#[derive(Debug, Error)]
pub enum SparseImageError {
    #[error("sparse image I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid sparse image header: {0}")]
    InvalidHeader(String),
    #[error("invalid sparse chunk {index}: {reason}")]
    InvalidChunk { index: u32, reason: String },
    #[error("sparse image arithmetic overflow while calculating {0}")]
    ArithmeticOverflow(&'static str),
    #[error("sparse image has no chunk covering logical offset {0}")]
    MissingChunk(u64),
    #[error("sparse image checksum mismatch: expected {expected:#010x}, computed {actual:#010x}")]
    ChecksumMismatch { expected: u32, actual: u32 },
    #[error("sparse image CRC32 chunk mismatch at logical offset {offset}: expected {expected:#010x}, computed {actual:#010x}")]
    Crc32Mismatch {
        offset: u64,
        expected: u32,
        actual: u32,
    },
}

impl SparseImageError {
    pub(crate) fn invalid_chunk(index: u32, reason: impl Into<String>) -> Self {
        Self::InvalidChunk {
            index,
            reason: reason.into(),
        }
    }
}
