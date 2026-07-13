#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RocksDbWireError {
    #[error("RocksDB log length {length} exceeds limit {limit}")]
    LogLengthLimit { length: usize, limit: usize },

    #[error("logical record length {length} exceeds limit {limit}")]
    LogicalRecordLengthLimit { length: usize, limit: usize },

    #[error("logical record count exceeds limit {limit}")]
    LogicalRecordCountLimit { limit: usize },

    #[error("truncated physical record header at offset {offset}: {available} bytes remain")]
    TruncatedLogHeader { offset: usize, available: usize },

    #[error(
        "physical record at offset {offset} crosses a 32 KiB block: header {header_size}, payload {payload_length}, block remainder {block_remaining}"
    )]
    CrossBlockRecord {
        offset: usize,
        header_size: usize,
        payload_length: usize,
        block_remaining: usize,
    },

    #[error(
        "truncated physical record body at offset {offset}: declared {declared}, available {available}"
    )]
    TruncatedLogBody {
        offset: usize,
        declared: usize,
        available: usize,
    },

    #[error("non-zero RocksDB block trailer begins at offset {offset}")]
    NonZeroLogTrailer { offset: usize },

    #[error("invalid RocksDB physical record type {record_type} at offset {offset}")]
    InvalidLogRecordType { offset: usize, record_type: u8 },

    #[error("WAL compression control record at offset {offset} is outside decoder scope")]
    UnsupportedWalCompressionRecord { offset: usize },

    #[error("invalid zero record at offset {offset}")]
    InvalidZeroRecord { offset: usize },

    #[error("recyclable record at offset {offset} requires an expected log number")]
    RecyclableLogNumberRequired { offset: usize },

    #[error(
        "recyclable record log number mismatch at offset {offset}: expected {expected}, got {actual}"
    )]
    RecyclableLogNumberMismatch {
        offset: usize,
        expected: u32,
        actual: u32,
    },

    #[error(
        "RocksDB physical record CRC32C mismatch at offset {offset}: expected {expected:#010x}, computed {actual:#010x}"
    )]
    LogCrcMismatch {
        offset: usize,
        expected: u32,
        actual: u32,
    },

    #[error("invalid fragment sequence at offset {offset}: expected {expected}, got {actual}")]
    InvalidFragmentSequence {
        offset: usize,
        expected: &'static str,
        actual: &'static str,
    },

    #[error("fragment encoding changed within a logical record at offset {offset}")]
    MixedFragmentEncoding { offset: usize },

    #[error("logical record beginning at offset {offset} has no LAST fragment")]
    UnterminatedLogicalRecord { offset: usize },

    #[error("length arithmetic overflow while decoding {context}")]
    LengthOverflow { context: &'static str },

    #[error("unexpected end of input at offset {offset} while decoding {context}")]
    UnexpectedEof {
        offset: usize,
        context: &'static str,
    },

    #[error("varint for {context} exceeds its {max_bytes}-byte wire limit at offset {offset}")]
    VarintTooLong {
        offset: usize,
        context: &'static str,
        max_bytes: usize,
    },

    #[error("varint for {context} overflows at offset {offset}")]
    VarintOverflow {
        offset: usize,
        context: &'static str,
    },

    #[error("non-canonical varint for {context} at offset {offset}")]
    NonCanonicalVarint {
        offset: usize,
        context: &'static str,
    },

    #[error("VersionEdit length {length} exceeds limit {limit}")]
    VersionEditLengthLimit { length: usize, limit: usize },

    #[error("{context} length {length} exceeds limit {limit}")]
    FieldLengthLimit {
        context: &'static str,
        length: usize,
        limit: usize,
    },

    #[error("VersionEdit tag count exceeds limit {limit}")]
    VersionEditTagLimit { limit: usize },

    #[error("duplicate singular VersionEdit field {field}")]
    DuplicateVersionEditField { field: &'static str },

    #[error("unknown mandatory VersionEdit tag {tag}")]
    UnknownMandatoryTag { tag: u32 },

    #[error("unknown mandatory NewFile4 custom tag {tag}")]
    UnknownMandatoryCustomTag { tag: u32 },

    #[error("NewFile4 custom field count exceeds limit {limit}")]
    CustomFieldCountLimit { limit: usize },

    #[error("duplicate NewFile4 custom field tag {tag}")]
    DuplicateCustomField { tag: u32 },

    #[error("invalid {context}: {reason}")]
    InvalidField {
        context: &'static str,
        reason: &'static str,
    },

    #[error("internal key for {context} is too short: {length} bytes")]
    InternalKeyTooShort {
        context: &'static str,
        length: usize,
    },

    #[error("internal key for {context} has unsupported value type {value_type:#04x}")]
    InvalidInternalKeyType {
        context: &'static str,
        value_type: u8,
    },

    #[error("level {level} exceeds configured maximum {max_level}")]
    InvalidLevel { level: u32, max_level: u32 },

    #[error("invalid RocksDB file number {file_number}")]
    InvalidFileNumber { file_number: u64 },

    #[error("invalid RocksDB path ID {path_id}")]
    InvalidPathId { path_id: u32 },

    #[error("sequence number {sequence} exceeds RocksDB's 56-bit limit")]
    InvalidSequenceNumber { sequence: u64 },

    #[error("file sequence range is reversed: smallest {smallest}, largest {largest}")]
    InvalidSequenceRange { smallest: u64, largest: u64 },

    #[error("file mutation count exceeds limit {limit}")]
    FileMutationLimit { limit: usize },

    #[error("compact cursor count exceeds limit {limit}")]
    CompactCursorLimit { limit: usize },

    #[error("conflicting values for {field}: {first} then {second}")]
    ConflictingField {
        field: &'static str,
        first: u64,
        second: u64,
    },

    #[error("atomic group sequence is invalid at edit {ordinal}: {reason}")]
    InvalidAtomicGroup { ordinal: u64, reason: &'static str },

    #[error("manifest is missing required recovery field {field}")]
    MissingRecoveryField { field: &'static str },

    #[error("column family {column_family_id} is missing or dropped at edit {ordinal}")]
    MissingColumnFamily { ordinal: u64, column_family_id: u32 },

    #[error("column family conflict for ID {column_family_id} at edit {ordinal}: {reason}")]
    ColumnFamilyConflict {
        ordinal: u64,
        column_family_id: u32,
        reason: &'static str,
    },

    #[error("column family count exceeds limit {limit}")]
    ColumnFamilyLimit { limit: usize },

    #[error("live file count exceeds limit {limit}")]
    LiveFileLimit { limit: usize },

    #[error("live file conflict for file {file_number} at edit {ordinal}: {reason}")]
    LiveFileConflict {
        ordinal: u64,
        file_number: u64,
        reason: &'static str,
    },

    #[error(
        "deleted live file is missing at edit {ordinal}: CF {column_family_id}, level {level}, file {file_number}"
    )]
    MissingLiveFile {
        ordinal: u64,
        column_family_id: u32,
        level: u32,
        file_number: u64,
    },

    #[error("{field} decreased at edit {ordinal}: previous {previous}, current {current}")]
    NonMonotonicField {
        ordinal: u64,
        field: &'static str,
        previous: u64,
        current: u64,
    },
}

pub type Result<T> = std::result::Result<T, RocksDbWireError>;
