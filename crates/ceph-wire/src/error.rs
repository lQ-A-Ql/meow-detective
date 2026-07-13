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

    #[error("integer overflow while decoding {context}")]
    IntegerOverflow { context: &'static str },

    #[error("varint for {context} exceeds the {limit}-byte limit")]
    VarintTooLong { context: &'static str, limit: usize },

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

    #[error("invalid BlueFS superblock size: expected {expected} bytes, got {actual}")]
    InvalidBluefsSuperblockSize { expected: usize, actual: usize },

    #[error(
        "BlueFS superblock CRC32C mismatch: expected {expected:#010x}, computed {actual:#010x}"
    )]
    BluefsCrcMismatch { expected: u32, actual: u32 },

    #[error("BlueFS fnode encoding {encoding} is invalid")]
    InvalidBluefsFnodeEncoding { encoding: u64 },

    #[error("BlueFS extent length {length} is invalid")]
    InvalidBluefsExtentLength { length: u64 },

    #[error("BlueFS {context} boolean has invalid wire value {value}")]
    InvalidBluefsBoolean { context: &'static str, value: u8 },

    #[error("BlueFS block size {block_size} is invalid")]
    InvalidBluefsBlockSize { block_size: u32 },

    #[error("BlueFS transaction payload length {length} exceeds limit {limit}")]
    BluefsTransactionLengthLimit { length: usize, limit: usize },

    #[error(
        "BlueFS transaction payload length {payload_length} is shorter than the declared minimum {minimum_length}"
    )]
    BluefsTransactionPayloadLengthMismatch {
        payload_length: usize,
        minimum_length: usize,
    },

    #[error("BlueFS transaction operation CRC32C mismatch: expected {expected:#010x}, computed {actual:#010x}")]
    BluefsTransactionCrcMismatch { expected: u32, actual: u32 },

    #[error("unknown BlueFS transaction operation {opcode}")]
    UnknownBluefsOperation { opcode: u8 },

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
