use crate::checksum::f2fs_crc32;
use crate::io::{read_exact_at, read_u16, read_u32, read_u64, SharedReader};
use crate::{F2fsError, Result, F2FS_BLOCK_SIZE, F2FS_MAGIC};

const SUPERBLOCK_OFFSET: u64 = 1024;
const SUPERBLOCK_BYTES: usize = 3072;
const SUPERBLOCK_CHECKSUM_OFFSET: usize = 3068;
const FEATURE_SUPERBLOCK_CHECKSUM: u32 = 0x0000_0800;
const FEATURE_PACKED_SSA: u32 = 0x0001_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuperblockCopy {
    Primary,
    Backup,
}

#[derive(Debug, Clone)]
pub struct F2fsSuperblock {
    pub source_copy: SuperblockCopy,
    pub major_version: u16,
    pub minor_version: u16,
    pub blocks_per_segment: u32,
    pub block_count: u64,
    pub cp_block: u32,
    pub nat_block: u32,
    pub main_block: u32,
    pub root_inode: u32,
    pub cp_payload_blocks: u32,
    pub feature_flags: u32,
}

impl F2fsSuperblock {
    pub(crate) fn read(source: &SharedReader, volume_offset: u64) -> Result<Self> {
        let primary = volume_offset
            .checked_add(SUPERBLOCK_OFFSET)
            .ok_or_else(|| F2fsError::Invalid("superblock offset overflows".to_string()))?;
        match read_copy(source, primary, SuperblockCopy::Primary) {
            Ok(superblock) => Ok(superblock),
            Err(primary_error) => {
                let backup = primary.checked_add(F2FS_BLOCK_SIZE as u64).ok_or_else(|| {
                    F2fsError::Invalid("backup superblock offset overflows".to_string())
                })?;
                read_copy(source, backup, SuperblockCopy::Backup).map_err(|backup_error| {
                    F2fsError::from_failed_copies("superblock", primary_error, backup_error)
                })
            }
        }
    }
}

fn read_copy(
    source: &SharedReader,
    offset: u64,
    source_copy: SuperblockCopy,
) -> Result<F2fsSuperblock> {
    let bytes = read_exact_at(source, offset, SUPERBLOCK_BYTES)?;
    if read_u32(&bytes, 0, "superblock magic")? != F2FS_MAGIC {
        return Err(F2fsError::Invalid("F2FS magic mismatch".to_string()));
    }
    let log_sector = read_u32(&bytes, 8, "sector size")?;
    let log_sectors_per_block = read_u32(&bytes, 12, "sectors per block")?;
    let log_block = read_u32(&bytes, 16, "block size")?;
    if log_block != 12 || log_sector.checked_add(log_sectors_per_block) != Some(log_block) {
        return Err(F2fsError::Invalid(
            "F2FS requires a consistent 4096-byte block geometry".to_string(),
        ));
    }
    let log_blocks_per_segment = read_u32(&bytes, 20, "blocks per segment")?;
    if log_blocks_per_segment != 9 {
        return Err(F2fsError::Unsupported(format!(
            "segment geometry 2^{log_blocks_per_segment} blocks"
        )));
    }
    let feature_flags = read_u32(&bytes, 2180, "feature flags")?;
    if feature_flags & FEATURE_SUPERBLOCK_CHECKSUM != 0 {
        let expected = read_u32(&bytes, SUPERBLOCK_CHECKSUM_OFFSET, "superblock checksum")?;
        let actual = f2fs_crc32(F2FS_MAGIC, &bytes[..SUPERBLOCK_CHECKSUM_OFFSET]);
        if actual != expected {
            return Err(F2fsError::Invalid(format!(
                "superblock checksum mismatch: expected {expected:#010x}, computed {actual:#010x}"
            )));
        }
    }
    let superblock = F2fsSuperblock {
        source_copy,
        major_version: read_u16(&bytes, 4, "major version")?,
        minor_version: read_u16(&bytes, 6, "minor version")?,
        blocks_per_segment: 1u32 << log_blocks_per_segment,
        block_count: read_u64(&bytes, 36, "block count")?,
        cp_block: read_u32(&bytes, 76, "checkpoint address")?,
        nat_block: read_u32(&bytes, 84, "NAT address")?,
        main_block: read_u32(&bytes, 92, "main area address")?,
        root_inode: read_u32(&bytes, 96, "root inode")?,
        cp_payload_blocks: read_u32(&bytes, 1664, "checkpoint payload")?,
        feature_flags,
    };
    validate_layout(&superblock)?;
    Ok(superblock)
}

fn validate_layout(superblock: &F2fsSuperblock) -> Result<()> {
    if superblock.root_inode < 3 {
        return Err(F2fsError::Invalid("reserved root inode number".to_string()));
    }
    if !(superblock.cp_block < superblock.nat_block
        && superblock.nat_block < superblock.main_block
        && u64::from(superblock.main_block) < superblock.block_count)
    {
        return Err(F2fsError::Invalid(
            "checkpoint, NAT, and main areas are not strictly ordered".to_string(),
        ));
    }
    if superblock.cp_payload_blocks != 0 {
        return Err(F2fsError::Unsupported(
            "checkpoint payload blocks are not implemented".to_string(),
        ));
    }
    if superblock.feature_flags & FEATURE_PACKED_SSA != 0 {
        return Err(F2fsError::Unsupported(
            "packed SSA summary layout".to_string(),
        ));
    }
    Ok(())
}
