use super::descriptor::verify_metadata_block_checksum;
use super::error::{require_len, JournalError, JournalResult};
use super::types::{
    read_be_u32, read_be_u64, JournalBlockType, JournalHeader, JournalSuperblock, RevokeBlock,
    JBD2_FEATURE_INCOMPAT_REVOKE,
};

const REVOKE_HEADER_SIZE: usize = 16;

pub fn parse_revoke_block(
    data: &[u8],
    superblock: &JournalSuperblock,
) -> JournalResult<RevokeBlock> {
    let block_size = superblock.block_size as usize;
    require_len(data, block_size, "revoke block")?;
    let data = &data[..block_size];
    let header = JournalHeader::parse(data)?;
    if header.block_type != JournalBlockType::Revoke {
        return Err(JournalError::Invalid(format!(
            "block type {:?} is not a revoke block",
            header.block_type
        )));
    }
    if !superblock.has_incompat(JBD2_FEATURE_INCOMPAT_REVOKE) {
        return Err(JournalError::Invalid(
            "revoke block is present without the REVOKE feature".into(),
        ));
    }
    let (usable_end, checksum) = verify_metadata_block_checksum(data, superblock)?;
    let bytes_used = read_be_u32(data, 12, "revoke byte count")?;
    let bytes_used_usize = usize::try_from(bytes_used)
        .map_err(|_| JournalError::Invalid("revoke byte count exceeds usize".into()))?;
    if bytes_used_usize < REVOKE_HEADER_SIZE || bytes_used_usize > usable_end {
        return Err(JournalError::Invalid(format!(
            "revoke byte count {bytes_used} is outside {REVOKE_HEADER_SIZE}..={usable_end}"
        )));
    }
    let record_size = if superblock.has_64bit_block_numbers() {
        8
    } else {
        4
    };
    let record_bytes = bytes_used_usize - REVOKE_HEADER_SIZE;
    if !record_bytes.is_multiple_of(record_size) {
        return Err(JournalError::Invalid(format!(
            "revoke records use {record_bytes} bytes, not a multiple of {record_size}"
        )));
    }
    let mut revoked_blocks = Vec::with_capacity(record_bytes / record_size);
    let mut cursor = REVOKE_HEADER_SIZE;
    while cursor < bytes_used_usize {
        let block = if record_size == 8 {
            read_be_u64(data, cursor, "64-bit revoke record")?
        } else {
            u64::from(read_be_u32(data, cursor, "32-bit revoke record")?)
        };
        revoked_blocks.push(block);
        cursor += record_size;
    }
    Ok(RevokeBlock {
        header,
        bytes_used,
        revoked_blocks,
        checksum,
    })
}
