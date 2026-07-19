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

    #[error("invalid BlueStore semantic key in {key_space}: {reason}")]
    InvalidBlueStoreSemanticKey {
        key_space: &'static str,
        reason: &'static str,
    },

    #[error("invalid BlueStore OMAP key in {family}: {reason}")]
    InvalidBlueStoreOmapKey {
        family: &'static str,
        reason: &'static str,
    },

    #[error("invalid BlueStore semantic value in {context}: {reason}")]
    InvalidBlueStoreSemanticValue {
        context: &'static str,
        reason: &'static str,
    },

    #[error("unknown BlueStore blob flag bits {unknown_bits:#x} in encoded flags {flags:#x}")]
    UnknownBlueStoreBlobFlags { flags: u32, unknown_bits: u32 },

    #[error("unknown BlueStore checksum type {checksum_type}")]
    UnknownBlueStoreChecksumType { checksum_type: u8 },

    #[error("invalid BlueStore checksum metadata for type {checksum_type}: {reason}")]
    InvalidBlueStoreChecksum {
        checksum_type: u8,
        reason: &'static str,
    },

    #[error(
        "invalid BlueStore physical extent {index}: offset {offset:#x}, length {length:#x}: {reason}"
    )]
    InvalidBlueStorePhysicalExtent {
        index: usize,
        offset: u64,
        length: u32,
        reason: &'static str,
    },

    #[error("duplicate BlueStore {kind} blob id {blob_id}")]
    DuplicateBlueStoreBlob { kind: &'static str, blob_id: u64 },

    #[error("BlueStore extent {record_index} references missing {kind} blob id {blob_id}")]
    MissingBlueStoreBlobReference {
        record_index: u32,
        kind: &'static str,
        blob_id: u64,
    },

    #[error("BlueStore spanning blob context is not bound to the extent-shard object")]
    BlueStoreSpanningBlobOwnerMismatch,

    #[error("BlueStore extent count mismatch: declared {declared}, decoded {decoded}")]
    BlueStoreExtentCountMismatch { declared: u32, decoded: u32 },

    #[error(
        "BlueStore logical extent at {logical_offset:#x} overlaps the previous end {previous_end:#x}"
    )]
    BlueStoreLogicalExtentOverlap {
        previous_end: u64,
        logical_offset: u64,
    },

    #[error(
        "BlueStore extent {record_index} range {blob_offset:#x}~{length:#x} exceeds blob logical length {logical_length:#x}"
    )]
    BlueStoreBlobRangeOverflow {
        record_index: u32,
        blob_offset: u32,
        length: u32,
        logical_length: u32,
    },

    #[error("invalid BlueStore extent {record_index}: {reason}")]
    InvalidBlueStoreExtent {
        record_index: u32,
        reason: &'static str,
    },

    #[error(
        "unsupported BlueStore DENC version {encoded_version} for {context}; supported versions are {supported_versions}"
    )]
    UnsupportedBlueStoreDencVersion {
        context: &'static str,
        encoded_version: u8,
        supported_versions: &'static str,
    },

    #[error("trailing bytes after {context}: {remaining} bytes remain")]
    BlueStoreTrailingBytes {
        context: &'static str,
        remaining: usize,
    },

    #[error("invalid RBD metadata {field}: {reason}")]
    InvalidRbdMetadata {
        field: &'static str,
        reason: &'static str,
    },

    #[error("trailing bytes after RBD metadata {field}: {remaining} bytes remain")]
    RbdTrailingBytes {
        field: &'static str,
        remaining: usize,
    },

    #[error("invalid RBD head-image layout: {reason}")]
    InvalidRbdLayout { reason: &'static str },

    #[error("RBD logical range {offset:#x}~{length:#x} is outside image size {image_size:#x}")]
    RbdRangeOutOfBounds {
        offset: u64,
        length: u64,
        image_size: u64,
    },

    #[error("RBD logical range arithmetic overflow at {offset:#x}~{length:#x}")]
    RbdRangeOverflow { offset: u64, length: u64 },

    #[error(
        "unsupported CephFS {map} version {encoded_version} (supported {minimum_version}..={decoder_version})"
    )]
    UnsupportedCephFsMapVersion {
        map: &'static str,
        encoded_version: u8,
        minimum_version: u8,
        decoder_version: u8,
    },

    #[error("invalid CephFS {map} field {field}: {reason}")]
    InvalidCephFsMap {
        map: &'static str,
        field: &'static str,
        reason: &'static str,
    },

    #[error("duplicate CephFS {kind} identifier {value}")]
    DuplicateCephFsIdentifier { kind: &'static str, value: i64 },

    #[error("unknown CephFS MDS daemon state {value}")]
    UnknownCephFsMdsState { value: i32 },

    #[error("trailing bytes after CephFS {map}: {remaining} bytes remain")]
    CephFsTrailingBytes { map: &'static str, remaining: usize },
}

pub type Result<T> = std::result::Result<T, CephWireError>;
