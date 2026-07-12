use crate::block_cache::BlockCache;
use crate::format::{
    inode_table_block_from_descriptor, EXT4_64BIT_GROUP_DESCRIPTOR_SIZE,
    EXT4_FEATURE_INCOMPAT_64BIT, EXT4_MAGIC, EXT4_METADATA_CACHE_BYTES,
    EXT4_MIN_GROUP_DESCRIPTOR_SIZE, EXT4_SUPERBLOCK_OFFSET, I_BLOCK_SIZE,
};
use evidence_core::filesystem::invalid_fs_data;
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;

pub struct Ext4Reader {
    pub(crate) reader: RefCell<Box<dyn EvidenceReader>>,
    pub(crate) block_size: u64,
    pub(crate) inode_size: u16,
    pub(crate) inodes_per_group: u32,
    pub(crate) bg_desc_table_block: u64,
    pub(crate) group_descriptor_size: u16,
    pub(crate) has_64bit: bool,
    pub(crate) num_block_groups: u32,
    pub(crate) volume_offset: u64,
    pub(crate) metadata_block_cache: RefCell<BlockCache>,
}

impl Ext4Reader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        let superblock_offset = offset
            .checked_add(EXT4_SUPERBLOCK_OFFSET)
            .ok_or_else(|| invalid_fs_data("ext4 superblock offset overflows"))?;
        reader.seek(SeekFrom::Start(superblock_offset))?;
        let mut superblock = [0u8; 1024];
        reader.read_exact(&mut superblock)?;

        let magic = u16::from_le_bytes([superblock[0x38], superblock[0x39]]);
        if magic != EXT4_MAGIC {
            return Err(invalid_fs_data(format!(
                "not a valid ext4 filesystem (magic 0x{:04X})",
                magic
            )));
        }

        let log_block_size = read_u32(&superblock, 0x18)?;
        if log_block_size > 6 {
            return Err(invalid_fs_data(format!(
                "unsupported ext4 log block size {}",
                log_block_size
            )));
        }
        let block_size = 1024u64
            .checked_shl(log_block_size)
            .ok_or_else(|| invalid_fs_data("ext4 block size overflows"))?;

        let inodes_count = read_u32(&superblock, 0x00)?;
        let blocks_count_lo = read_u32(&superblock, 0x04)?;
        let blocks_per_group = read_u32(&superblock, 0x20)?;
        let inodes_per_group = read_u32(&superblock, 0x28)?;
        let first_data_block = read_u32(&superblock, 0x14)?;
        let raw_inode_size = u16::from_le_bytes([superblock[0x58], superblock[0x59]]);
        let inode_size = if raw_inode_size == 0 {
            128
        } else {
            raw_inode_size
        };
        let feature_incompat = read_u32(&superblock, 0x60)?;
        let has_64bit = feature_incompat & EXT4_FEATURE_INCOMPAT_64BIT != 0;
        let blocks_count_hi = if has_64bit {
            read_u32(&superblock, 0x150)?
        } else {
            0
        };
        let blocks_count = blocks_count_lo as u64 | ((blocks_count_hi as u64) << u32::BITS);
        let raw_descriptor_size = u16::from_le_bytes([superblock[0xFE], superblock[0xFF]]);
        let group_descriptor_size = if has_64bit {
            raw_descriptor_size
        } else {
            EXT4_MIN_GROUP_DESCRIPTOR_SIZE
        };

        validate_geometry(
            block_size,
            blocks_count,
            blocks_per_group,
            inodes_per_group,
            inode_size,
            has_64bit,
            group_descriptor_size,
        )?;
        let block_group_count = blocks_count
            .saturating_sub(first_data_block as u64)
            .div_ceil(blocks_per_group as u64);
        let inode_group_count = (inodes_count as u64).div_ceil(inodes_per_group as u64);
        let num_block_groups = u32::try_from(block_group_count.max(inode_group_count))
            .map_err(|_| invalid_fs_data("ext4 block group count exceeds u32"))?;

        Ok(Self {
            reader: RefCell::new(reader),
            block_size,
            inode_size,
            inodes_per_group,
            bg_desc_table_block: first_data_block as u64 + 1,
            group_descriptor_size,
            has_64bit,
            num_block_groups,
            volume_offset: offset,
            metadata_block_cache: RefCell::new(BlockCache::with_byte_budget(
                block_size,
                EXT4_METADATA_CACHE_BYTES,
            )),
        })
    }

    pub(crate) fn block_to_offset(&self, block: u64) -> io::Result<u64> {
        block
            .checked_mul(self.block_size)
            .and_then(|offset| self.volume_offset.checked_add(offset))
            .ok_or_else(|| invalid_fs_data(format!("ext4 block {} offset overflows", block)))
    }

    pub(crate) fn read_block(&self, block: u64) -> io::Result<Vec<u8>> {
        self.read_bytes_at(self.block_to_offset(block)?, self.block_size as usize)
    }

    pub(crate) fn read_metadata_block(&self, block: u64) -> io::Result<Arc<[u8]>> {
        if let Some(cached) = self.metadata_block_cache.borrow().get(block) {
            return Ok(cached);
        }
        let data = self.read_block(block)?;
        Ok(self.metadata_block_cache.borrow_mut().insert(block, data))
    }

    pub(crate) fn read_bytes_at(&self, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; length];
        if length == 0 {
            return Ok(buf);
        }
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub(crate) fn read_bg_descriptor(&self, group: u32) -> io::Result<Vec<u8>> {
        let descriptor_size = self.group_descriptor_size as u64;
        let descriptors_per_block = self.block_size / descriptor_size;
        let descriptor_block = self
            .bg_desc_table_block
            .checked_add(group as u64 / descriptors_per_block)
            .ok_or_else(|| invalid_fs_data("ext4 descriptor table block overflows"))?;
        let offset_in_block = (group as u64 % descriptors_per_block) * descriptor_size;
        let block_data = self.read_metadata_block(descriptor_block)?;
        let start = offset_in_block as usize;
        let end = start
            .checked_add(self.group_descriptor_size as usize)
            .ok_or_else(|| invalid_fs_data("ext4 group descriptor offset overflows"))?;
        if end > block_data.len() {
            return Err(invalid_fs_data(format!(
                "ext4 group descriptor {} exceeds descriptor table block",
                group
            )));
        }
        Ok(block_data[start..end].to_vec())
    }

    pub(crate) fn read_inode(&self, inode_number: u32) -> io::Result<Vec<u8>> {
        if inode_number < 1 {
            return Err(invalid_fs_data("inode number must be >= 1"));
        }
        let group = (inode_number - 1) / self.inodes_per_group;
        let local_index = (inode_number - 1) % self.inodes_per_group;
        if group >= self.num_block_groups {
            return Err(invalid_fs_data(format!(
                "inode {} belongs to non-existent block group {}",
                inode_number, group
            )));
        }
        let descriptor = self.read_bg_descriptor(group)?;
        let table_block = inode_table_block_from_descriptor(&descriptor, self.has_64bit)?;
        let byte_offset = local_index as u64 * self.inode_size as u64;
        let inode_block = table_block
            .checked_add(byte_offset / self.block_size)
            .ok_or_else(|| invalid_fs_data(format!("inode {} block overflows", inode_number)))?;
        let offset_in_block = (byte_offset % self.block_size) as usize;
        self.read_inode_bytes(inode_number, inode_block, offset_in_block)
    }

    fn read_inode_bytes(
        &self,
        inode_number: u32,
        inode_block: u64,
        offset_in_block: usize,
    ) -> io::Result<Vec<u8>> {
        let inode_size = self.inode_size as usize;
        let first_block = self.read_metadata_block(inode_block)?;
        let first_len = inode_size.min(first_block.len().saturating_sub(offset_in_block));
        if first_len == 0 {
            return Err(invalid_fs_data(format!(
                "inode {} offset exceeds inode-table block",
                inode_number
            )));
        }
        let mut inode = Vec::with_capacity(inode_size);
        inode.extend_from_slice(&first_block[offset_in_block..offset_in_block + first_len]);
        if inode.len() < inode_size {
            let continuation = inode_block.checked_add(1).ok_or_else(|| {
                invalid_fs_data(format!(
                    "inode {} continuation block overflows",
                    inode_number
                ))
            })?;
            let second_block = self.read_metadata_block(continuation)?;
            let remaining = inode_size - inode.len();
            if remaining > second_block.len() {
                return Err(invalid_fs_data(format!(
                    "inode {} spans beyond the next inode-table block",
                    inode_number
                )));
            }
            inode.extend_from_slice(&second_block[..remaining]);
        }
        Ok(inode)
    }

    pub fn inode_mode(inode: &[u8]) -> u16 {
        u16::from_le_bytes([inode[0], inode[1]])
    }

    pub fn inode_size(inode: &[u8]) -> io::Result<u64> {
        let low = u32::from_le_bytes(
            inode[0x04..0x08]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        ) as u64;
        if inode.len() > 0x70 {
            let high = u32::from_le_bytes(
                inode[0x6C..0x70]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            ) as u64;
            Ok(low | (high << 32))
        } else {
            Ok(low)
        }
    }

    pub fn inode_i_block(inode: &[u8]) -> &[u8] {
        let start = 0x28;
        let end = (start + I_BLOCK_SIZE).min(inode.len());
        &inode[start..end]
    }
}

fn read_u32(data: &[u8], offset: usize) -> io::Result<u32> {
    Ok(u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .map_err(|_| invalid_fs_data("disk parse error"))?,
    ))
}

fn validate_geometry(
    block_size: u64,
    blocks_count: u64,
    blocks_per_group: u32,
    inodes_per_group: u32,
    inode_size: u16,
    has_64bit: bool,
    descriptor_size: u16,
) -> io::Result<()> {
    if blocks_count == 0 || blocks_per_group == 0 || inodes_per_group == 0 {
        return Err(invalid_fs_data("invalid ext4 geometry"));
    }
    if inode_size < 128 || !inode_size.is_power_of_two() || inode_size as u64 > block_size {
        return Err(invalid_fs_data(format!(
            "invalid ext4 inode size {} for block size {}",
            inode_size, block_size
        )));
    }
    if has_64bit && descriptor_size < EXT4_64BIT_GROUP_DESCRIPTOR_SIZE {
        return Err(invalid_fs_data(format!(
            "64-bit ext4 group descriptor size {} is smaller than {} bytes",
            descriptor_size, EXT4_64BIT_GROUP_DESCRIPTOR_SIZE
        )));
    }
    if descriptor_size < EXT4_MIN_GROUP_DESCRIPTOR_SIZE
        || descriptor_size as u64 > block_size
        || !block_size.is_multiple_of(descriptor_size as u64)
    {
        return Err(invalid_fs_data(format!(
            "invalid ext4 group descriptor size {} for block size {}",
            descriptor_size, block_size
        )));
    }
    Ok(())
}
