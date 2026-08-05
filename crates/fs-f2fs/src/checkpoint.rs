use std::collections::HashMap;

use crate::checksum::f2fs_crc32;
use crate::io::{block_offset, read_exact_at, read_u32, read_u64, SharedReader};
use crate::{F2fsError, F2fsSuperblock, Result, F2FS_BLOCK_SIZE, F2FS_MAGIC};

const CHECKPOINT_BITMAP_OFFSET: usize = 192;
const CHECKSUM_OFFSET_FIELD: usize = 164;
const MIN_CHECKSUM_OFFSET: usize = CHECKPOINT_BITMAP_OFFSET;
const MAX_CHECKSUM_OFFSET: usize = F2FS_BLOCK_SIZE - 4;
const CP_LARGE_NAT_BITMAP_FLAG: u32 = 0x0000_0400;
const CP_COMPACT_SUM_FLAG: u32 = 0x0000_0004;
const CP_UMOUNT_FLAG: u32 = 0x0000_0001;
const SUMMARY_JOURNAL_OFFSET: usize = 3584;
const SUMMARY_JOURNAL_BYTES: usize = 507;
const NAT_JOURNAL_ENTRY_BYTES: usize = 13;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CheckpointNatEntry {
    pub(crate) inode: u32,
    pub(crate) block: u32,
}

pub(crate) struct Checkpoint {
    pub(crate) version: u64,
    pub(crate) nat_bitmap: Vec<u8>,
    pub(crate) nat_journal: HashMap<u32, CheckpointNatEntry>,
}

impl Checkpoint {
    pub(crate) fn read(
        source: &SharedReader,
        volume_offset: u64,
        superblock: &F2fsSuperblock,
    ) -> Result<Self> {
        let primary = read_pack(source, volume_offset, superblock, superblock.cp_block);
        let backup_block = superblock
            .cp_block
            .checked_add(superblock.blocks_per_segment)
            .ok_or_else(|| F2fsError::Invalid("backup checkpoint address overflows".to_string()))?;
        let backup = read_pack(source, volume_offset, superblock, backup_block);
        match (primary, backup) {
            (Ok(primary), Ok(backup)) => {
                if version_after(backup.version, primary.version) {
                    Ok(backup)
                } else {
                    Ok(primary)
                }
            }
            (Ok(primary), Err(_)) => Ok(primary),
            (Err(_), Ok(backup)) => Ok(backup),
            (Err(primary), Err(backup)) => Err(F2fsError::from_failed_copies(
                "checkpoint pack",
                primary,
                backup,
            )),
        }
    }
}

fn read_pack(
    source: &SharedReader,
    volume_offset: u64,
    superblock: &F2fsSuperblock,
    start_block: u32,
) -> Result<Checkpoint> {
    let first = read_checkpoint_block(source, volume_offset, start_block)?;
    let total_blocks = read_u32(&first, 136, "checkpoint pack block count")?;
    if total_blocks <= 2 || total_blocks > superblock.blocks_per_segment {
        return Err(F2fsError::Invalid(format!(
            "invalid checkpoint pack block count {total_blocks}"
        )));
    }
    let last_block = start_block
        .checked_add(total_blocks - 1)
        .ok_or_else(|| F2fsError::Invalid("checkpoint tail address overflows".to_string()))?;
    let last = read_checkpoint_block(source, volume_offset, last_block)?;
    let version = read_u64(&first, 0, "checkpoint version")?;
    if read_u64(&last, 0, "checkpoint tail version")? != version {
        return Err(F2fsError::Invalid(
            "checkpoint head and tail versions differ".to_string(),
        ));
    }
    let flags = read_u32(&first, 132, "checkpoint flags")?;
    let sit_size = read_u32(&first, 156, "SIT bitmap size")? as usize;
    let nat_size = read_u32(&first, 160, "NAT bitmap size")? as usize;
    let checksum_offset =
        read_u32(&first, CHECKSUM_OFFSET_FIELD, "checkpoint checksum offset")? as usize;
    let checkpoint = read_checkpoint_region(
        source,
        volume_offset,
        start_block,
        &first,
        superblock.cp_payload_blocks,
    )?;
    let nat_bitmap = extract_nat_bitmap(
        &checkpoint,
        sit_size,
        nat_size,
        checksum_offset,
        superblock.cp_payload_blocks,
        flags,
    )?;
    let start_sum = read_u32(&first, 140, "checkpoint summary start")?;
    let nat_journal = read_nat_journal(
        source,
        volume_offset,
        start_block,
        total_blocks,
        start_sum,
        flags,
    )?;
    Ok(Checkpoint {
        version,
        nat_bitmap,
        nat_journal,
    })
}

fn read_checkpoint_region(
    source: &SharedReader,
    volume_offset: u64,
    start_block: u32,
    first: &[u8],
    payload_blocks: u32,
) -> Result<Vec<u8>> {
    let capacity = usize::try_from(payload_blocks + 1)
        .ok()
        .and_then(|blocks| blocks.checked_mul(F2FS_BLOCK_SIZE))
        .ok_or_else(|| F2fsError::Invalid("checkpoint payload size overflows".to_string()))?;
    let mut checkpoint = Vec::with_capacity(capacity);
    checkpoint.extend_from_slice(first);
    for relative in 1..=payload_blocks {
        let block = start_block.checked_add(relative).ok_or_else(|| {
            F2fsError::Invalid("checkpoint payload address overflows".to_string())
        })?;
        checkpoint.extend_from_slice(&read_exact_at(
            source,
            block_offset(volume_offset, block)?,
            F2FS_BLOCK_SIZE,
        )?);
    }
    Ok(checkpoint)
}

fn extract_nat_bitmap(
    checkpoint: &[u8],
    sit_size: usize,
    nat_size: usize,
    checksum_offset: usize,
    payload_blocks: u32,
    flags: u32,
) -> Result<Vec<u8>> {
    let large = flags & CP_LARGE_NAT_BITMAP_FLAG != 0;
    let start = if large {
        if checksum_offset != CHECKPOINT_BITMAP_OFFSET {
            return Err(F2fsError::Invalid(format!(
                "large NAT bitmap checksum offset {checksum_offset} is not {CHECKPOINT_BITMAP_OFFSET}"
            )));
        }
        let start = CHECKPOINT_BITMAP_OFFSET + 4;
        validate_bitmap_end(checkpoint, start, nat_size, sit_size)?;
        start
    } else if payload_blocks != 0 {
        validate_bitmap_end(checkpoint, F2FS_BLOCK_SIZE, sit_size, 0)?;
        validate_checksum_bound(CHECKPOINT_BITMAP_OFFSET, nat_size, checksum_offset)?;
        CHECKPOINT_BITMAP_OFFSET
    } else {
        let start = CHECKPOINT_BITMAP_OFFSET
            .checked_add(sit_size)
            .ok_or_else(|| F2fsError::Invalid("checkpoint bitmap offset overflows".to_string()))?;
        validate_checksum_bound(start, nat_size, checksum_offset)?;
        start
    };
    let end = start
        .checked_add(nat_size)
        .ok_or_else(|| F2fsError::Invalid("NAT bitmap length overflows".to_string()))?;
    checkpoint
        .get(start..end)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| F2fsError::Invalid("NAT bitmap exceeds checkpoint payload".to_string()))
}

fn validate_bitmap_end(
    checkpoint: &[u8],
    start: usize,
    first_size: usize,
    second_size: usize,
) -> Result<()> {
    let end = start
        .checked_add(first_size)
        .and_then(|value| value.checked_add(second_size))
        .ok_or_else(|| F2fsError::Invalid("checkpoint bitmap range overflows".to_string()))?;
    if end > checkpoint.len() {
        return Err(F2fsError::Invalid(
            "checkpoint bitmaps exceed payload blocks".to_string(),
        ));
    }
    Ok(())
}

fn validate_checksum_bound(start: usize, length: usize, checksum_offset: usize) -> Result<()> {
    if start
        .checked_add(length)
        .is_none_or(|end| end > checksum_offset)
    {
        return Err(F2fsError::Invalid(
            "checkpoint bitmap overlaps its checksum".to_string(),
        ));
    }
    Ok(())
}

fn read_nat_journal(
    source: &SharedReader,
    volume_offset: u64,
    start_block: u32,
    total_blocks: u32,
    start_sum: u32,
    flags: u32,
) -> Result<HashMap<u32, CheckpointNatEntry>> {
    let (relative_block, offset) = if flags & CP_COMPACT_SUM_FLAG != 0 {
        (start_sum, 0usize)
    } else {
        let summary_count = if flags & CP_UMOUNT_FLAG != 0 { 7 } else { 4 };
        let relative = total_blocks.checked_sub(summary_count).ok_or_else(|| {
            F2fsError::Invalid("checkpoint pack is too short for data summaries".to_string())
        })?;
        (relative, SUMMARY_JOURNAL_OFFSET)
    };
    let block = start_block
        .checked_add(relative_block)
        .ok_or_else(|| F2fsError::Invalid("summary block address overflows".to_string()))?;
    let bytes = read_exact_at(source, block_offset(volume_offset, block)?, F2FS_BLOCK_SIZE)?;
    let journal = bytes
        .get(offset..offset + SUMMARY_JOURNAL_BYTES)
        .ok_or_else(|| F2fsError::Invalid("NAT journal exceeds summary block".to_string()))?;
    parse_nat_journal(journal)
}

fn parse_nat_journal(bytes: &[u8]) -> Result<HashMap<u32, CheckpointNatEntry>> {
    let count = u16::from_le_bytes(
        bytes
            .get(..2)
            .ok_or_else(|| F2fsError::Invalid("truncated NAT journal count".to_string()))?
            .try_into()
            .map_err(|_| F2fsError::Invalid("truncated NAT journal count".to_string()))?,
    ) as usize;
    let capacity = (bytes.len() - 2) / NAT_JOURNAL_ENTRY_BYTES;
    if count > capacity {
        return Err(F2fsError::Invalid(format!(
            "NAT journal count {count} exceeds capacity {capacity}"
        )));
    }
    let mut entries = HashMap::with_capacity(count);
    for index in 0..count {
        let offset = 2 + index * NAT_JOURNAL_ENTRY_BYTES;
        let nid = read_u32(bytes, offset, "NAT journal nid")?;
        let inode = read_u32(bytes, offset + 5, "NAT journal inode")?;
        let block = read_u32(bytes, offset + 9, "NAT journal block")?;
        entries.insert(nid, CheckpointNatEntry { inode, block });
    }
    Ok(entries)
}

fn read_checkpoint_block(source: &SharedReader, volume_offset: u64, block: u32) -> Result<Vec<u8>> {
    let bytes = read_exact_at(source, block_offset(volume_offset, block)?, F2FS_BLOCK_SIZE)?;
    let checksum_offset =
        read_u32(&bytes, CHECKSUM_OFFSET_FIELD, "checkpoint checksum offset")? as usize;
    if !(MIN_CHECKSUM_OFFSET..=MAX_CHECKSUM_OFFSET).contains(&checksum_offset) {
        return Err(F2fsError::Invalid(format!(
            "checkpoint checksum offset {checksum_offset} is invalid"
        )));
    }
    let expected = read_u32(&bytes, checksum_offset, "checkpoint checksum")?;
    let actual = f2fs_crc32(F2FS_MAGIC, &bytes[..checksum_offset]);
    if actual != expected {
        return Err(F2fsError::Invalid(format!(
            "checkpoint checksum mismatch: expected {expected:#010x}, computed {actual:#010x}"
        )));
    }
    Ok(bytes)
}

fn version_after(candidate: u64, current: u64) -> bool {
    let delta = candidate.wrapping_sub(current);
    delta != 0 && delta < (1u64 << 63)
}
