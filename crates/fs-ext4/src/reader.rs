use crate::block_cache::BlockCache;
use crate::format::{inode_table_block_from_descriptor, EXT4_METADATA_CACHE_BYTES, I_BLOCK_SIZE};
use crate::superblock::Ext4Superblock;
use evidence_core::filesystem::invalid_fs_data;
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;

pub struct Ext4Reader {
    pub(crate) reader: RefCell<Box<dyn EvidenceReader>>,
    pub(crate) block_size: u64,
    pub(crate) blocks_count: u64,
    pub(crate) blocks_per_group: u32,
    pub(crate) first_data_block: u64,
    pub(crate) inode_size: u16,
    pub(crate) inodes_per_group: u32,
    pub(crate) inodes_count: u32,
    pub(crate) filesystem_uuid: [u8; 16],
    pub(crate) bg_desc_table_block: u64,
    pub(crate) group_descriptor_size: u16,
    pub(crate) has_64bit: bool,
    pub(crate) has_bigalloc: bool,
    pub(crate) has_gdt_csum: bool,
    pub(crate) has_metadata_csum: bool,
    pub(crate) checksum_seed: u32,
    pub(crate) num_block_groups: u32,
    pub(crate) has_journal: bool,
    pub(crate) journal_inode: Option<u32>,
    pub(crate) volume_offset: u64,
    pub(crate) metadata_block_cache: RefCell<BlockCache>,
}

impl Ext4Reader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        let superblock = Ext4Superblock::read(reader.as_mut(), offset)?;

        Ok(Self {
            reader: RefCell::new(reader),
            block_size: superblock.block_size,
            blocks_count: superblock.blocks_count,
            blocks_per_group: superblock.blocks_per_group,
            first_data_block: superblock.first_data_block,
            inode_size: superblock.inode_size,
            inodes_per_group: superblock.inodes_per_group,
            inodes_count: superblock.inodes_count,
            filesystem_uuid: superblock.filesystem_uuid,
            bg_desc_table_block: superblock.first_data_block + 1,
            group_descriptor_size: superblock.group_descriptor_size,
            has_64bit: superblock.has_64bit,
            has_bigalloc: superblock.has_bigalloc,
            has_gdt_csum: superblock.has_gdt_csum,
            has_metadata_csum: superblock.has_metadata_csum,
            checksum_seed: superblock.checksum_seed,
            num_block_groups: superblock.num_block_groups,
            has_journal: superblock.has_journal,
            journal_inode: superblock.journal_inode,
            volume_offset: offset,
            metadata_block_cache: RefCell::new(BlockCache::with_byte_budget(
                superblock.block_size,
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
