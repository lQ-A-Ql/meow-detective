use uuid::Uuid;

/// Errors produced while decoding or selecting Ceph wire data.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CephWireError {
    #[error("unexpected end of input at offset {offset}: need {needed} bytes, have {remaining}")]
    UnexpectedEof {
        offset: usize,
        needed: usize,
        remaining: usize,
    },

    #[error("length {length} for {context} exceeds limit {limit}")]
    LengthLimit {
        context: &'static str,
        length: usize,
        limit: usize,
    },

    #[error("length arithmetic overflow while decoding {context}")]
    LengthOverflow { context: &'static str },

    #[error("invalid UTF-8 in {context}: {message}")]
    InvalidUtf8 {
        context: &'static str,
        message: String,
    },

    #[error(
        "unsupported Ceph struct envelope: decoder version {decoder_version}, encoded version {encoded_version}, compat {compat_version}"
    )]
    IncompatibleStructVersion {
        decoder_version: u8,
        encoded_version: u8,
        compat_version: u8,
    },

    #[error("Ceph struct payload ended at {struct_end}, but decoder reached {offset}")]
    StructBoundaryExceeded { struct_end: usize, offset: usize },

    #[error("BlueStore label CRC32C mismatch: expected {expected:#010x}, computed {actual:#010x}")]
    CrcMismatch { expected: u32, actual: u32 },

    #[error("BlueStore label metadata epoch is invalid: {value}")]
    InvalidEpoch { value: String },

    #[error(
        "conflicting BlueStore label copies for OSD {osd_uuid} at epoch {epoch}: positions {first_position:#x} and {conflicting_position:#x}"
    )]
    ConflictingLabelCopies {
        osd_uuid: Uuid,
        epoch: i64,
        first_position: u64,
        conflicting_position: u64,
    },

    #[error("no valid BlueStore bdev label matched the requested UUID")]
    NoValidLabel,
}

pub type Result<T> = std::result::Result<T, CephWireError>;
