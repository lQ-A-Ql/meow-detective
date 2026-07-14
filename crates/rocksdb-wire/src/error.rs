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

    #[error("WriteBatch length {length} exceeds limit {limit}")]
    WriteBatchLengthLimit { length: usize, limit: usize },

    #[error("WriteBatch mutation count {count} exceeds limit {limit}")]
    WriteBatchMutationLimit { count: u32, limit: usize },

    #[error("WriteBatch auxiliary record count exceeds limit {limit}")]
    WriteBatchAuxiliaryRecordLimit { limit: usize },

    #[error("WriteBatch declared {declared} mutations but decoded {decoded}")]
    WriteBatchCountMismatch { declared: u32, decoded: u32 },

    #[error("unsupported RocksDB WriteBatch tag {tag:#04x} at offset {offset}")]
    UnsupportedWriteBatchTag { offset: usize, tag: u8 },

    #[error("invalid RocksDB WriteBatch tag {tag:#04x} at offset {offset}")]
    InvalidWriteBatchTag { offset: usize, tag: u8 },

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

    #[error("tracked WAL VersionEdit tag {tag} is unsupported")]
    UnsupportedTrackedWalEdit { tag: u32 },

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

    #[error("SST range read failed at offset {offset} for {length} bytes")]
    SstRangeRead { offset: u64, length: usize },

    #[error("SST evidence source read failed at offset {offset} for {length} bytes")]
    SstSourceRead { offset: u64, length: usize },

    #[error("SST inspection was cancelled")]
    SstInspectionCancelled,

    #[error("SST file size {file_size} is smaller than the {minimum} byte minimum")]
    SstFileTooShort { file_size: u64, minimum: u64 },

    #[error("unsupported RocksDB SST table magic {magic:#018x}")]
    UnsupportedSstMagic { magic: u64 },

    #[error("unsupported RocksDB SST format version {version}")]
    UnsupportedSstFormatVersion { version: u32 },

    #[error("unsupported RocksDB SST checksum type {checksum_type:#04x}")]
    UnsupportedSstChecksum { checksum_type: u8 },

    #[error("invalid RocksDB SST footer padding at byte {offset}")]
    NonZeroSstFooterPadding { offset: usize },

    #[error("invalid {context} block handle: {reason}")]
    InvalidBlockHandle {
        context: &'static str,
        reason: &'static str,
    },

    #[error("SST block range {offset}..{end} is outside the {boundary} byte structural boundary")]
    SstBlockOutOfRange {
        offset: u64,
        end: u64,
        boundary: u64,
    },

    #[error("SST stored block size {size} exceeds limit {limit}")]
    SstStoredBlockLimit { size: u64, limit: usize },

    #[error("SST decompressed block size {size} exceeds limit {limit}")]
    SstDecompressedBlockLimit { size: usize, limit: usize },

    #[error("SST auxiliary metadata size {total} exceeds cumulative limit {limit}")]
    SstAuxiliaryMetadataLimit { total: u64, limit: usize },

    #[error("SST block checksum mismatch at offset {offset}: expected {expected:#010x}, computed {actual:#010x}")]
    SstChecksumMismatch {
        offset: u64,
        expected: u32,
        actual: u32,
    },

    #[error("unsupported SST block compression type {compression_type:#04x} at offset {offset}")]
    UnsupportedSstCompression { offset: u64, compression_type: u8 },

    #[error("SST LZ4 block at offset {offset} is malformed: {reason}")]
    InvalidSstCompression { offset: u64, reason: &'static str },

    #[error("SST restart block is malformed: {reason}")]
    InvalidRestartBlock { reason: &'static str },

    #[error("SST restart block entry count exceeds limit {limit}")]
    SstEntryLimit { limit: usize },

    #[error("SST key-space census entry count exceeds limit {limit}")]
    SstCensusEntryLimit { limit: u64 },

    #[error("SST key-space census decompressed bytes exceed limit {limit}")]
    SstCensusDecompressedLimit { limit: u64 },

    #[error("SST entry stream data block count {count} exceeds limit {limit}")]
    SstStreamDataBlockLimit { count: u64, limit: u64 },

    #[error("SST entry stream total entry count exceeds limit {limit}")]
    SstStreamEntryLimit { limit: u64 },

    #[error("SST entry stream range deletion count exceeds limit {limit}")]
    SstStreamRangeDeletionLimit { limit: u64 },

    #[error("SST entry stream decompressed bytes exceed limit {limit}")]
    SstStreamDecompressedLimit { limit: u64 },

    #[error("SST block key length {length} exceeds limit {limit}")]
    SstKeyLengthLimit { length: usize, limit: usize },

    #[error("SST block value length {length} exceeds limit {limit}")]
    SstValueLengthLimit { length: usize, limit: usize },

    #[error("duplicate SST metaindex entry")]
    DuplicateMetaBlock,

    #[error("invalid SST metaindex entry: {reason}")]
    InvalidMetaIndex { reason: &'static str },

    #[error("SST is missing required properties block")]
    MissingPropertiesBlock,

    #[error("duplicate SST property")]
    DuplicateSstProperty,

    #[error("missing required SST property {name}")]
    MissingSstProperty { name: &'static str },

    #[error("invalid {context}: {reason}")]
    InvalidSstProperty {
        context: &'static str,
        reason: &'static str,
    },

    #[error("unsupported SST table feature {feature}: {value}")]
    UnsupportedSstFeature { feature: &'static str, value: u64 },

    #[error("unsupported SST internal entry type {value_type:#04x}")]
    UnsupportedSstEntryType { value_type: u8 },

    #[error("SST index is invalid: {reason}")]
    InvalidSstIndex { reason: &'static str },

    #[error("SST data counts do not match properties for {field}: parsed {parsed}, properties {properties}")]
    SstCountMismatch {
        field: &'static str,
        parsed: u64,
        properties: u64,
    },

    #[error("invalid sanitized SST census context: {reason}")]
    InvalidSstCensusContext { reason: &'static str },

    #[error("SST column family does not match the supplied census context")]
    SstCensusColumnFamilyMismatch,
}

pub type Result<T> = std::result::Result<T, RocksDbWireError>;
