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

    /// A committed metadata copy was readable but structurally invalid.
    #[error("fatal metadata parse error at line {line}: {message}")]
    FatalMetadataParseError { line: usize, message: String },

    #[error("conflicting committed metadata for volume group '{vg_uuid}' at sequence {seqno}")]
    ConflictingMetadata { vg_uuid: String, seqno: u64 },

    /// Unsupported segment type (striped, raid, thin, etc.).
    #[error("unsupported segment type '{seg_type}' in logical volume '{lv_name}'")]
    UnsupportedSegment { lv_name: String, seg_type: String },

    /// Referenced physical volume not found in volume group.
    #[error("unknown physical volume '{name}' referenced in segment mapping")]
    UnknownPhysicalVolume { name: String },

    /// Logical volume index out of bounds.
    #[error("logical volume index {index} out of range (0..{count})")]
    LvIndexOutOfRange { index: usize, count: usize },

    /// Thin snapshot whose unmapped blocks must fall back to an origin
    /// volume; that fallback is not implemented, so opening is refused.
    #[error(
        "thin logical volume '{lv_name}' references {origin_kind} '{origin}'; origin fallback reads are not supported"
    )]
    UnsupportedThinSnapshotOrigin {
        lv_name: String,
        origin_kind: &'static str,
        origin: String,
    },

    #[error("thin pool transaction mismatch: LVM expects {expected}, metadata contains {actual}")]
    ThinPoolTransactionMismatch { expected: u64, actual: u64 },

    #[error("thin pool transaction ID is missing from LVM metadata")]
    MissingThinPoolTransaction,

    #[error("thin metadata block {block} checksum mismatch: expected {expected:#010x}, computed {actual:#010x}")]
    ThinChecksum {
        block: u64,
        expected: u32,
        actual: u32,
    },

    #[error("unsupported thin metadata version {version}")]
    ThinVersion { version: u32 },

    #[error("thin metadata requires checking or has unsupported flags {flags:#010x}")]
    ThinFlags { flags: u32 },

    #[error("unsupported thin metadata incompatible features {flags:#010x}")]
    ThinFeatures { flags: u32 },

    #[error("thin device {device_id} transaction {actual} exceeds pool transaction {maximum}")]
    ThinDeviceTransaction {
        device_id: u64,
        actual: u64,
        maximum: u64,
    },

    #[error("thin device {device_id} maps {mapped_blocks} blocks but logical capacity is {logical_blocks}")]
    ThinDeviceCapacity {
        device_id: u64,
        mapped_blocks: u64,
        logical_blocks: u64,
    },

    /// Volume group metadata references a PV that does not have a supplied reader.
    #[error("missing reader for physical volume '{pv_name}' ({pv_uuid})")]
    MissingPhysicalVolumeReader { pv_name: String, pv_uuid: String },

    /// I/O error from underlying reader.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Convenience result type for LVM operations.
pub type Result<T> = std::result::Result<T, LvmError>;
