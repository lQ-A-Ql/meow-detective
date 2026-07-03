/// LVM2 parsing errors.
#[derive(Debug, thiserror::Error)]
pub enum LvmError {
    /// Sector does not contain an LVM2 label.
    #[error("not an LVM2 physical volume")]
    NotLvm,

    /// Label sector CRC-32 mismatch.
    #[error("label CRC mismatch: expected {expected:#010x}, computed {actual:#010x}")]
    LabelCrcMismatch { expected: u32, actual: u32 },

    /// MDA header CRC-32 mismatch.
    #[error(
        "metadata area header CRC mismatch: expected {expected:#010x}, computed {actual:#010x}"
    )]
    MdaCrcMismatch { expected: u32, actual: u32 },

    /// Metadata text block CRC-32 mismatch.
    #[error("metadata text CRC mismatch (copy {index}): expected {expected:#010x}, computed {actual:#010x}")]
    MetadataCrcMismatch {
        index: usize,
        expected: u32,
        actual: u32,
    },

    /// Metadata text could not be parsed.
    #[error("metadata parse error at line {line}: {message}")]
    MetadataParseError { line: usize, message: String },

    /// Unsupported segment type (striped, raid, thin, etc.).
    #[error("unsupported segment type '{seg_type}' in logical volume '{lv_name}'")]
    UnsupportedSegment { lv_name: String, seg_type: String },

    /// Referenced physical volume not found in volume group.
    #[error("unknown physical volume '{name}' referenced in segment mapping")]
    UnknownPhysicalVolume { name: String },

    /// Logical volume index out of bounds.
    #[error("logical volume index {index} out of range (0..{count})")]
    LvIndexOutOfRange { index: usize, count: usize },

    /// I/O error from underlying reader.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Convenience result type for LVM operations.
pub type Result<T> = std::result::Result<T, LvmError>;
