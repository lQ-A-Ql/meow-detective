use std::io;

use thiserror::Error;

use crate::AndroidFilesystemKind;

pub type Result<T> = std::result::Result<T, VolumeAndroidError>;

#[derive(Debug, Error)]
pub enum VolumeAndroidError {
    #[error("Android dynamic-partition I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("truncated Android dynamic-partition field: {0}")]
    Truncated(&'static str),
    #[error("invalid Android dynamic-partition geometry: {0}")]
    InvalidGeometry(String),
    #[error("invalid Android dynamic-partition metadata: {0}")]
    InvalidMetadata(String),
    #[error("both geometry copies are invalid: primary={primary}; backup={backup}")]
    GeometryCopiesInvalid { primary: String, backup: String },
    #[error(
        "both metadata copies are invalid for slot {slot}: primary={primary}; backup={backup}"
    )]
    MetadataCopiesInvalid {
        slot: u32,
        primary: String,
        backup: String,
    },
    #[error("Android dynamic-partition arithmetic overflow while calculating {0}")]
    ArithmeticOverflow(&'static str),
    #[error(
        "logical partition `{partition}` requires unsupported block device index {source_index}"
    )]
    UnsupportedBlockDevice {
        partition: String,
        source_index: u32,
    },
    #[error("logical partition `{partition}` is disabled")]
    DisabledPartition { partition: String },
    #[error("logical partition has no extent covering offset {0}")]
    MissingExtent(u64),
    #[error("Android filesystem `{filesystem}` is recognized but its reader is not available")]
    UnsupportedFilesystem { filesystem: AndroidFilesystemKind },
    #[error("Android logical partition does not contain a recognized filesystem")]
    UnrecognizedFilesystem,
    #[error("failed to open Android {filesystem} filesystem reader: {message}")]
    FilesystemReaderOpen {
        filesystem: AndroidFilesystemKind,
        message: String,
    },
}
