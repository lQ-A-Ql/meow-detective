use crate::checksum::crc32c;
use crate::io::{read_exact_at, read_u16, read_u32, read_u64, SharedReader};
use crate::{ErofsError, Result, EROFS_BLOCK_SIZE, EROFS_MAGIC};

const SUPERBLOCK_OFFSET: u64 = 1024;
const SUPERBLOCK_BYTES: usize = 144;
const SUPERBLOCK_CHECKSUM_FEATURE: u32 = 0x0000_0001;
const FEATURE_INCOMPAT_LZ4_0PADDING: u32 = 0x0000_0001;
const FEATURE_INCOMPAT_CHUNKED_FILE: u32 = 0x0000_0004;
const FEATURE_INCOMPAT_48BIT: u32 = 0x0000_0080;
const FEATURE_INCOMPAT_SUPPORTED: u32 =
    FEATURE_INCOMPAT_LZ4_0PADDING | FEATURE_INCOMPAT_CHUNKED_FILE | FEATURE_INCOMPAT_48BIT;
const CHECKSUM_SEED: u32 = 0x5045_b54a;
const CHECKSUM_START: usize = 8;
const CHECKSUM_END: usize = 3072;

#[derive(Debug, Clone)]
pub struct ErofsSuperblock {
    pub block_size: usize,
    pub block_count: u64,
    pub meta_block: u64,
    pub root_nid: u64,
    pub feature_compat: u32,
    pub feature_incompat: u32,
}

impl ErofsSuperblock {
    pub(crate) fn read(source: &SharedReader, volume_offset: u64) -> Result<Self> {
        let bytes = read_exact_at(
            source,
            volume_offset
                .checked_add(SUPERBLOCK_OFFSET)
                .ok_or_else(|| ErofsError::Invalid("superblock offset overflows".to_string()))?,
            SUPERBLOCK_BYTES,
        )?;
        if read_u32(&bytes, 0, "superblock magic")? != EROFS_MAGIC {
            return Err(ErofsError::Invalid("EROFS magic mismatch".to_string()));
        }
        let block_size = 1usize
            .checked_shl(u32::from(*bytes.get(12).ok_or_else(|| {
                ErofsError::Invalid("truncated superblock block size".to_string())
            })?))
            .ok_or_else(|| ErofsError::Invalid("EROFS block size shift overflows".to_string()))?;
        if block_size != EROFS_BLOCK_SIZE {
            return Err(ErofsError::Unsupported(format!(
                "block size {block_size} bytes"
            )));
        }
        let extension_slots = usize::from(*bytes.get(13).ok_or_else(|| {
            ErofsError::Invalid("truncated superblock extension slots".to_string())
        })?);
        if 128usize
            .checked_add(extension_slots.saturating_mul(16))
            .is_none_or(|size| size > block_size)
        {
            return Err(ErofsError::Invalid(
                "superblock extension area exceeds its block".to_string(),
            ));
        }
        let feature_compat = read_u32(&bytes, 8, "compatible features")?;
        if feature_compat & SUPERBLOCK_CHECKSUM_FEATURE != 0 {
            validate_checksum(source, volume_offset)?;
        }
        let feature_incompat = read_u32(&bytes, 80, "incompatible features")?;
        if feature_incompat & !FEATURE_INCOMPAT_SUPPORTED != 0 {
            return Err(ErofsError::Unsupported(format!(
                "incompatible feature flags {:#x}",
                feature_incompat & !FEATURE_INCOMPAT_SUPPORTED
            )));
        }
        if bytes[90] != 0 {
            return Err(ErofsError::Unsupported(format!(
                "directory block shift {}",
                bytes[90]
            )));
        }
        let is_48bit = feature_incompat & FEATURE_INCOMPAT_48BIT != 0;
        let root_nid = if is_48bit {
            read_u64(&bytes, 112, "48-bit root nid")?
        } else {
            u64::from(read_u16(&bytes, 14, "root nid")?)
        };
        let block_count = u64::from(read_u32(&bytes, 36, "block count")?)
            | if is_48bit {
                u64::from(read_u16(&bytes, 14, "block count high bits")?) << 32
            } else {
                0
            };
        let superblock = Self {
            block_size,
            block_count,
            meta_block: u64::from(read_u32(&bytes, 40, "metadata block")?),
            root_nid,
            feature_compat,
            feature_incompat,
        };
        validate_layout(&superblock)
    }

    pub(crate) fn supports_chunked_files(&self) -> bool {
        self.feature_incompat & FEATURE_INCOMPAT_CHUNKED_FILE != 0
    }

    pub(crate) fn supports_lz4_compression(&self) -> bool {
        self.feature_incompat & FEATURE_INCOMPAT_LZ4_0PADDING != 0
    }
}

fn validate_checksum(source: &SharedReader, volume_offset: u64) -> Result<()> {
    let block = read_exact_at(
        source,
        volume_offset
            .checked_add(SUPERBLOCK_OFFSET)
            .ok_or_else(|| ErofsError::Invalid("checksum offset overflows".to_string()))?,
        EROFS_BLOCK_SIZE,
    )?;
    let expected = read_u32(&block, 4, "superblock checksum")?;
    let actual = crc32c(CHECKSUM_SEED, &block[CHECKSUM_START..CHECKSUM_END]);
    if expected != actual {
        return Err(ErofsError::Invalid(format!(
            "superblock checksum mismatch: expected {expected:#010x}, computed {actual:#010x}"
        )));
    }
    Ok(())
}

fn validate_layout(superblock: &ErofsSuperblock) -> Result<ErofsSuperblock> {
    if superblock.block_count == 0 || superblock.meta_block >= superblock.block_count {
        return Err(ErofsError::Invalid(
            "metadata block is outside the filesystem".to_string(),
        ));
    }
    if superblock.root_nid == 0 || superblock.root_nid >= (1u64 << 63) {
        return Err(ErofsError::Invalid("invalid root nid".to_string()));
    }
    Ok(superblock.clone())
}
