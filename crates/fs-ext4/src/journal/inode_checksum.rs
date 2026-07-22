use super::checksum::crc32c;
use super::error::{require_len, JournalError, JournalResult};
use crate::Ext4Reader;

const EXT4_GOOD_OLD_INODE_SIZE: usize = 128;
const INODE_GENERATION_OFFSET: usize = 0x64;
const INODE_CHECKSUM_LO_OFFSET: usize = 0x7C;
const INODE_EXTRA_ISIZE_OFFSET: usize = 0x80;
const INODE_CHECKSUM_HI_OFFSET: usize = 0x82;
const INODE_CHECKSUM_FIELD_SIZE: usize = 2;

pub(super) fn verify_inode_checksum(
    filesystem: &Ext4Reader,
    inode_number: u32,
    inode: &[u8],
) -> JournalResult<Option<bool>> {
    if !filesystem.has_metadata_csum {
        return Ok(None);
    }
    let inode_size = usize::from(filesystem.inode_size);
    require_len(inode, inode_size, "ext4 inode checksum")?;
    require_len(
        inode,
        INODE_CHECKSUM_LO_OFFSET + INODE_CHECKSUM_FIELD_SIZE,
        "ext4 inode checksum low field",
    )?;
    let has_high = inode_has_high_checksum(inode, inode_size)?;
    let provided_low = u32::from(read_le_u16(inode, INODE_CHECKSUM_LO_OFFSET)?);
    let provided = if has_high {
        provided_low | (u32::from(read_le_u16(inode, INODE_CHECKSUM_HI_OFFSET)?) << 16)
    } else {
        provided_low
    };

    let generation = inode
        .get(INODE_GENERATION_OFFSET..INODE_GENERATION_OFFSET + 4)
        .ok_or(JournalError::Truncated {
            context: "ext4 inode generation",
            needed: INODE_GENERATION_OFFSET + 4,
            available: inode.len(),
        })?;
    let mut checksum = crc32c(filesystem.checksum_seed, &inode_number.to_le_bytes());
    checksum = crc32c(checksum, generation);
    checksum = checksum_inode_bytes(checksum, &inode[..inode_size], has_high);
    if !has_high {
        checksum &= 0xFFFF;
    }
    Ok(Some(checksum == provided))
}

fn inode_has_high_checksum(inode: &[u8], inode_size: usize) -> JournalResult<bool> {
    if inode_size <= EXT4_GOOD_OLD_INODE_SIZE {
        return Ok(false);
    }
    let extra_isize = usize::from(read_le_u16(inode, INODE_EXTRA_ISIZE_OFFSET)?);
    let fields_end = EXT4_GOOD_OLD_INODE_SIZE
        .checked_add(extra_isize)
        .ok_or_else(|| JournalError::Invalid("ext4 inode extra size overflows".into()))?;
    Ok(
        INODE_CHECKSUM_HI_OFFSET + INODE_CHECKSUM_FIELD_SIZE <= fields_end
            && INODE_CHECKSUM_HI_OFFSET + INODE_CHECKSUM_FIELD_SIZE <= inode_size,
    )
}

fn checksum_inode_bytes(mut checksum: u32, inode: &[u8], has_high: bool) -> u32 {
    checksum = crc32c(checksum, &inode[..INODE_CHECKSUM_LO_OFFSET]);
    checksum = crc32c(checksum, &[0, 0]);
    checksum = crc32c(
        checksum,
        &inode[INODE_CHECKSUM_LO_OFFSET + INODE_CHECKSUM_FIELD_SIZE..EXT4_GOOD_OLD_INODE_SIZE],
    );
    if inode.len() == EXT4_GOOD_OLD_INODE_SIZE {
        return checksum;
    }
    checksum = crc32c(
        checksum,
        &inode[EXT4_GOOD_OLD_INODE_SIZE..INODE_CHECKSUM_HI_OFFSET],
    );
    if has_high {
        checksum = crc32c(checksum, &[0, 0]);
        crc32c(
            checksum,
            &inode[INODE_CHECKSUM_HI_OFFSET + INODE_CHECKSUM_FIELD_SIZE..],
        )
    } else {
        crc32c(checksum, &inode[INODE_CHECKSUM_HI_OFFSET..])
    }
}

fn read_le_u16(data: &[u8], offset: usize) -> JournalResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| JournalError::Invalid("ext4 inode checksum offset overflows".into()))?;
    let bytes = data.get(offset..end).ok_or(JournalError::Truncated {
        context: "ext4 inode checksum field",
        needed: end,
        available: data.len(),
    })?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}
