use super::checksum::crc32c_with_zeroed_range;
use super::error::{require_len, JournalError, JournalResult};

// On-disk constants mirror include/linux/jbd2.h; block type is never encoded in magic.
pub const JBD2_MAGIC_NUMBER: u32 = 0xC03B_3998;
pub const JOURNAL_INODE: u32 = 8;
pub const JOURNAL_HEADER_SIZE: usize = 12;
pub const JOURNAL_SUPERBLOCK_SIZE: usize = 1024;

pub const JBD2_FEATURE_COMPAT_CHECKSUM: u32 = 0x0000_0001;
pub const JBD2_FEATURE_INCOMPAT_REVOKE: u32 = 0x0000_0001;
pub const JBD2_FEATURE_INCOMPAT_64BIT: u32 = 0x0000_0002;
pub const JBD2_FEATURE_INCOMPAT_ASYNC_COMMIT: u32 = 0x0000_0004;
pub const JBD2_FEATURE_INCOMPAT_CSUM_V2: u32 = 0x0000_0008;
pub const JBD2_FEATURE_INCOMPAT_CSUM_V3: u32 = 0x0000_0010;
pub const JBD2_FEATURE_INCOMPAT_FAST_COMMIT: u32 = 0x0000_0020;

pub const JBD2_FLAG_ESCAPE: u32 = 0x0000_0001;
pub const JBD2_FLAG_SAME_UUID: u32 = 0x0000_0002;
pub const JBD2_FLAG_DELETED: u32 = 0x0000_0004;
pub const JBD2_FLAG_LAST_TAG: u32 = 0x0000_0008;

pub(crate) const JBD2_CRC32C_CHKSUM: u8 = 4;
pub(crate) const JBD2_DEFAULT_FAST_COMMIT_BLOCKS: u32 = 256;
pub(crate) const JBD2_KNOWN_INCOMPAT_FEATURES: u32 = JBD2_FEATURE_INCOMPAT_REVOKE
    | JBD2_FEATURE_INCOMPAT_64BIT
    | JBD2_FEATURE_INCOMPAT_ASYNC_COMMIT
    | JBD2_FEATURE_INCOMPAT_CSUM_V2
    | JBD2_FEATURE_INCOMPAT_CSUM_V3
    | JBD2_FEATURE_INCOMPAT_FAST_COMMIT;
pub(crate) const JBD2_KNOWN_RO_COMPAT_FEATURES: u32 = 0;
pub(crate) const JBD2_KNOWN_TAG_FLAGS: u32 =
    JBD2_FLAG_ESCAPE | JBD2_FLAG_SAME_UUID | JBD2_FLAG_DELETED | JBD2_FLAG_LAST_TAG;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum JournalBlockType {
    Descriptor = 1,
    Commit = 2,
    SuperblockV1 = 3,
    SuperblockV2 = 4,
    Revoke = 5,
}

impl TryFrom<u32> for JournalBlockType {
    type Error = JournalError;

    fn try_from(value: u32) -> JournalResult<Self> {
        match value {
            1 => Ok(Self::Descriptor),
            2 => Ok(Self::Commit),
            3 => Ok(Self::SuperblockV1),
            4 => Ok(Self::SuperblockV2),
            5 => Ok(Self::Revoke),
            _ => Err(JournalError::Invalid(format!(
                "unknown journal block type {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalSuperblockVersion {
    V1,
    V2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalTagFormat {
    Legacy32,
    Legacy64,
    ChecksumV2_32,
    ChecksumV2_64,
    ChecksumV3,
}

impl JournalTagFormat {
    /// Matches Linux `journal_tag_bytes()`, including checksum-v2 padding.
    pub fn byte_len(self) -> usize {
        match self {
            Self::Legacy32 => 8,
            Self::Legacy64 => 12,
            Self::ChecksumV2_32 => 10,
            Self::ChecksumV2_64 => 14,
            Self::ChecksumV3 => 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalHeader {
    pub block_type: JournalBlockType,
    pub sequence: u32,
}

impl JournalHeader {
    pub fn parse(data: &[u8]) -> JournalResult<Self> {
        require_len(data, JOURNAL_HEADER_SIZE, "common header")?;
        let magic = read_be_u32(data, 0, "common header magic")?;
        if magic != JBD2_MAGIC_NUMBER {
            return Err(JournalError::Invalid(format!(
                "invalid journal magic 0x{magic:08X}"
            )));
        }
        Ok(Self {
            block_type: JournalBlockType::try_from(read_be_u32(
                data,
                4,
                "common header block type",
            )?)?,
            sequence: read_be_u32(data, 8, "common header sequence")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalSuperblock {
    pub header: JournalHeader,
    pub version: JournalSuperblockVersion,
    pub block_size: u32,
    pub max_len: u32,
    pub first: u32,
    pub sequence: u32,
    pub start: u32,
    pub errno: u32,
    pub feature_compat: u32,
    pub feature_incompat: u32,
    pub feature_ro_compat: u32,
    pub uuid: [u8; 16],
    pub checksum_type: u8,
    pub num_fast_commit_blocks: u32,
    pub head: u32,
    pub checksum: Option<u32>,
}

impl JournalSuperblock {
    pub fn parse(data: &[u8]) -> JournalResult<Self> {
        require_len(data, JOURNAL_SUPERBLOCK_SIZE, "superblock")?;
        let data = &data[..JOURNAL_SUPERBLOCK_SIZE];
        let header = JournalHeader::parse(data)?;
        let version = match header.block_type {
            JournalBlockType::SuperblockV1 => JournalSuperblockVersion::V1,
            JournalBlockType::SuperblockV2 => JournalSuperblockVersion::V2,
            other => {
                return Err(JournalError::Invalid(format!(
                    "block type {other:?} is not a journal superblock"
                )))
            }
        };
        let mut superblock = Self {
            header,
            version,
            block_size: read_be_u32(data, 0x0C, "superblock block size")?,
            max_len: read_be_u32(data, 0x10, "superblock maximum length")?,
            first: read_be_u32(data, 0x14, "superblock first log block")?,
            sequence: read_be_u32(data, 0x18, "superblock sequence")?,
            start: read_be_u32(data, 0x1C, "superblock start block")?,
            errno: read_be_u32(data, 0x20, "superblock errno")?,
            feature_compat: 0,
            feature_incompat: 0,
            feature_ro_compat: 0,
            uuid: [0; 16],
            checksum_type: 0,
            num_fast_commit_blocks: 0,
            head: 0,
            checksum: None,
        };
        if version == JournalSuperblockVersion::V2 {
            superblock.feature_compat = read_be_u32(data, 0x24, "compatible features")?;
            superblock.feature_incompat = read_be_u32(data, 0x28, "incompatible features")?;
            superblock.feature_ro_compat = read_be_u32(data, 0x2C, "read-only features")?;
            superblock.uuid.copy_from_slice(&data[0x30..0x40]);
            superblock.checksum_type = data[0x50];
            superblock.num_fast_commit_blocks = read_be_u32(data, 0x54, "fast-commit block count")?;
            superblock.head = read_be_u32(data, 0x58, "journal head")?;
        }
        superblock.validate(data)?;
        Ok(superblock)
    }

    pub fn has_incompat(&self, feature: u32) -> bool {
        self.feature_incompat & feature != 0
    }

    pub fn uses_v2_or_v3_checksums(&self) -> bool {
        self.has_incompat(JBD2_FEATURE_INCOMPAT_CSUM_V2)
            || self.has_incompat(JBD2_FEATURE_INCOMPAT_CSUM_V3)
    }

    pub fn has_64bit_block_numbers(&self) -> bool {
        self.has_incompat(JBD2_FEATURE_INCOMPAT_64BIT)
    }

    pub fn tag_format(&self) -> JournalTagFormat {
        if self.has_incompat(JBD2_FEATURE_INCOMPAT_CSUM_V3) {
            return JournalTagFormat::ChecksumV3;
        }
        match (
            self.has_incompat(JBD2_FEATURE_INCOMPAT_CSUM_V2),
            self.has_64bit_block_numbers(),
        ) {
            (true, true) => JournalTagFormat::ChecksumV2_64,
            (true, false) => JournalTagFormat::ChecksumV2_32,
            (false, true) => JournalTagFormat::Legacy64,
            (false, false) => JournalTagFormat::Legacy32,
        }
    }

    pub fn log_last_exclusive(&self) -> JournalResult<u32> {
        if !self.has_incompat(JBD2_FEATURE_INCOMPAT_FAST_COMMIT) {
            return Ok(self.max_len);
        }
        let fast_commit_blocks = if self.num_fast_commit_blocks == 0 {
            JBD2_DEFAULT_FAST_COMMIT_BLOCKS
        } else {
            self.num_fast_commit_blocks
        };
        self.max_len
            .checked_sub(fast_commit_blocks)
            .filter(|last| *last > self.first)
            .ok_or_else(|| {
                JournalError::Invalid("fast-commit area consumes the normal journal ring".into())
            })
    }

    fn validate(&mut self, data: &[u8]) -> JournalResult<()> {
        if !(1024..=65_536).contains(&self.block_size) || !self.block_size.is_power_of_two() {
            return Err(JournalError::Invalid(format!(
                "invalid journal block size {}",
                self.block_size
            )));
        }
        if self.first == 0 || self.first >= self.max_len {
            return Err(JournalError::Invalid(format!(
                "invalid journal ring bounds first={} max_len={}",
                self.first, self.max_len
            )));
        }
        if self.version == JournalSuperblockVersion::V1 {
            return self.validate_start();
        }
        let unknown_incompat = self.feature_incompat & !JBD2_KNOWN_INCOMPAT_FEATURES;
        let unknown_ro = self.feature_ro_compat & !JBD2_KNOWN_RO_COMPAT_FEATURES;
        if unknown_incompat != 0 || unknown_ro != 0 {
            return Err(JournalError::Unsupported(format!(
                "unknown feature bits incompat=0x{unknown_incompat:08X}, ro=0x{unknown_ro:08X}"
            )));
        }
        let csum_v2 = self.has_incompat(JBD2_FEATURE_INCOMPAT_CSUM_V2);
        let csum_v3 = self.has_incompat(JBD2_FEATURE_INCOMPAT_CSUM_V3);
        if csum_v2 && csum_v3 {
            return Err(JournalError::Invalid(
                "checksum v2 and checksum v3 are mutually exclusive".into(),
            ));
        }
        if (csum_v2 || csum_v3) && self.feature_compat & JBD2_FEATURE_COMPAT_CHECKSUM != 0 {
            return Err(JournalError::Invalid(
                "legacy and v2/v3 checksum features are mutually exclusive".into(),
            ));
        }
        if self.uses_v2_or_v3_checksums() {
            if self.checksum_type != JBD2_CRC32C_CHKSUM {
                return Err(JournalError::Unsupported(format!(
                    "checksum type {} is not CRC32C",
                    self.checksum_type
                )));
            }
            let stored = read_be_u32(data, 0xFC, "superblock checksum")?;
            let calculated = crc32c_with_zeroed_range(u32::MAX, data, 0xFC..0x100)
                .ok_or_else(|| JournalError::Invalid("invalid checksum field range".into()))?;
            if stored != calculated {
                return Err(JournalError::Invalid(format!(
                    "superblock checksum mismatch: stored=0x{stored:08X}, calculated=0x{calculated:08X}"
                )));
            }
            self.checksum = Some(stored);
        }
        self.validate_start()
    }

    fn validate_start(&self) -> JournalResult<()> {
        let last = self.log_last_exclusive()?;
        if self.start != 0 && (self.start < self.first || self.start >= last) {
            return Err(JournalError::Invalid(format!(
                "journal start {} is outside ring {}..{}",
                self.start, self.first, last
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalTagChecksum {
    V2(u16),
    V3(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTag {
    pub target_block: u64,
    pub flags: u32,
    pub checksum: Option<JournalTagChecksum>,
    pub uuid: [u8; 16],
}

impl BlockTag {
    pub fn is_escaped(&self) -> bool {
        self.flags & JBD2_FLAG_ESCAPE != 0
    }

    pub fn is_last(&self) -> bool {
        self.flags & JBD2_FLAG_LAST_TAG != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptorBlock {
    pub header: JournalHeader,
    pub tags: Vec<BlockTag>,
    pub checksum: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokeBlock {
    pub header: JournalHeader,
    pub bytes_used: u32,
    pub revoked_blocks: Vec<u64>,
    pub checksum: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitBlock {
    pub header: JournalHeader,
    pub checksum_type: u8,
    pub checksum_size: u8,
    pub checksum: Option<u32>,
    pub commit_seconds: u64,
    pub commit_nanoseconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalBlockMapping {
    pub transaction_sequence: u32,
    pub descriptor_journal_block: u32,
    pub payload_journal_block: u32,
    pub target_filesystem_block: u64,
    pub flags: u32,
    pub uuid: [u8; 16],
    pub checksum: Option<JournalTagChecksum>,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalDescriptor {
    pub journal_block: u32,
    pub descriptor: DescriptorBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalRevoke {
    pub journal_block: u32,
    pub revoke: RevokeBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalCommit {
    pub journal_block: u32,
    pub commit: CommitBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalTransaction {
    pub sequence: u32,
    pub start_journal_block: u32,
    pub next_journal_block: u32,
    pub descriptors: Vec<JournalDescriptor>,
    pub mappings: Vec<JournalBlockMapping>,
    pub revokes: Vec<JournalRevoke>,
    pub commit: JournalCommit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompleteTransaction {
    pub sequence: u32,
    pub start_journal_block: u32,
    pub stopped_at_journal_block: u32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalScan {
    pub superblock: JournalSuperblock,
    pub transactions: Vec<JournalTransaction>,
    pub incomplete_transaction: Option<IncompleteTransaction>,
    pub scanned_ring_blocks: u32,
    pub next_journal_block: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalScanIssue {
    pub journal_block: u32,
    pub sequence: Option<u32>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalHistoryScan {
    pub superblock: JournalSuperblock,
    pub transactions: Vec<JournalTransaction>,
    pub rejected_candidates: Vec<JournalScanIssue>,
    pub scanned_ring_blocks: u32,
}

pub(crate) fn read_be_u16(data: &[u8], offset: usize, context: &'static str) -> JournalResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| JournalError::Invalid(format!("{context} offset overflows")))?;
    require_len(data, end, context)?;
    Ok(u16::from_be_bytes([data[offset], data[offset + 1]]))
}

pub(crate) fn read_be_u32(data: &[u8], offset: usize, context: &'static str) -> JournalResult<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| JournalError::Invalid(format!("{context} offset overflows")))?;
    require_len(data, end, context)?;
    Ok(u32::from_be_bytes(data[offset..end].try_into().map_err(
        |_| JournalError::Invalid(format!("invalid {context}")),
    )?))
}

pub(crate) fn read_be_u64(data: &[u8], offset: usize, context: &'static str) -> JournalResult<u64> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| JournalError::Invalid(format!("{context} offset overflows")))?;
    require_len(data, end, context)?;
    Ok(u64::from_be_bytes(data[offset..end].try_into().map_err(
        |_| JournalError::Invalid(format!("invalid {context}")),
    )?))
}
