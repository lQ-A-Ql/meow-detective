use super::checksum::{crc32c_with_zeroed_range, journal_checksum_seed};
use super::error::{require_len, JournalError, JournalResult};
use super::types::{
    read_be_u32, read_be_u64, CommitBlock, JournalBlockType, JournalHeader, JournalSuperblock,
    JBD2_CRC32C_CHKSUM,
};

const COMMIT_HEADER_SIZE: usize = 60;
const COMMIT_CHECKSUM_OFFSET: usize = 16;
const COMMIT_CHECKSUM_END: usize = 20;

pub fn parse_commit_block(
    data: &[u8],
    superblock: &JournalSuperblock,
) -> JournalResult<CommitBlock> {
    let block_size = superblock.block_size as usize;
    require_len(data, block_size, "commit block")?;
    require_len(data, COMMIT_HEADER_SIZE, "commit header")?;
    let data = &data[..block_size];
    let header = JournalHeader::parse(data)?;
    if header.block_type != JournalBlockType::Commit {
        return Err(JournalError::Invalid(format!(
            "block type {:?} is not a commit block",
            header.block_type
        )));
    }
    let checksum_type = data[12];
    let checksum_size = data[13];
    let checksum = if superblock.uses_v2_or_v3_checksums() {
        if checksum_type != JBD2_CRC32C_CHKSUM || checksum_size != 4 {
            return Err(JournalError::Invalid(format!(
                "invalid commit checksum descriptor type={checksum_type} size={checksum_size}"
            )));
        }
        let stored = read_be_u32(data, COMMIT_CHECKSUM_OFFSET, "commit checksum")?;
        let calculated = crc32c_with_zeroed_range(
            journal_checksum_seed(&superblock.uuid),
            data,
            COMMIT_CHECKSUM_OFFSET..COMMIT_CHECKSUM_END,
        )
        .ok_or_else(|| JournalError::Invalid("invalid commit checksum range".into()))?;
        if stored != calculated {
            return Err(JournalError::Invalid(format!(
                "commit checksum mismatch: stored=0x{stored:08X}, calculated=0x{calculated:08X}"
            )));
        }
        Some(stored)
    } else {
        None
    };
    let commit_nanoseconds = read_be_u32(data, 56, "commit nanoseconds")?;
    if commit_nanoseconds >= 1_000_000_000 {
        return Err(JournalError::Invalid(format!(
            "commit nanoseconds {commit_nanoseconds} exceed one second"
        )));
    }
    Ok(CommitBlock {
        header,
        checksum_type,
        checksum_size,
        checksum,
        commit_seconds: read_be_u64(data, 48, "commit seconds")?,
        commit_nanoseconds,
    })
}
