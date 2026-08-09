//! File-to-physical-offset mapping for host-side in-place edits.
//!
//! The emulation overlay writer edits file content in place (same bytes
//! over the same blocks) and shrinks `/etc/shadow` by rewriting the inode
//! size. Both need the physical location of a file's extents and of its
//! inode record. Everything here is computed read-only; only the caller's
//! overlay disk is ever written.

use std::io;

use evidence_core::filesystem::invalid_fs_data;

use crate::format::{inode_table_block_from_descriptor, Ext4Extent, Ext4ExtentHeader};

/// One contiguous byte range of a file, expressed in the reader's
/// coordinate space (including any volume base the reader was opened with).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ext4FileExtent {
    pub logical_offset: u64,
    pub volume_offset: u64,
    pub length: u64,
}

const I_FLAGS_OFFSET: usize = 0x20;
const I_SIZE_LO_OFFSET: usize = 0x04;
const I_SIZE_HI_OFFSET: usize = 0x6C;
const EXT4_EXTENTS_FL: u32 = 0x0008_0000;
const EXT4_ENCRYPT_FL: u32 = 0x0000_0800;
const EXT4_VERITY_FL: u32 = 0x0000_8000;
const EXT4_INLINE_DATA_FL: u32 = 0x1000_0000;

impl crate::Ext4Reader {
    /// Map a regular file's extents to physical byte ranges. Files without
    /// the extents feature, with inline data, or with encryption/verity
    /// cannot be edited in place and are refused.
    pub fn file_extent_map(&self, path: &str) -> io::Result<Vec<Ext4FileExtent>> {
        let inode_number = self.regular_file_inode(path)?;
        let inode = self.read_inode(inode_number)?;
        let flags = read_u32(&inode, I_FLAGS_OFFSET)?;
        if flags & EXT4_EXTENTS_FL == 0 {
            return Err(invalid_fs_data(format!(
                "{path} does not use extents; refusing to map it"
            )));
        }
        if flags & (EXT4_INLINE_DATA_FL | EXT4_ENCRYPT_FL | EXT4_VERITY_FL) != 0 {
            return Err(invalid_fs_data(format!(
                "{path} is inline/encrypted/verity; refusing to edit it in place"
            )));
        }
        let mut extents = Vec::new();
        self.collect_extents(Self::inode_i_block(&inode), &mut extents)?;
        Ok(extents)
    }

    /// The file's logical size (`i_size`), needed to bound the rewrite.
    pub fn file_size_by_path(&self, path: &str) -> io::Result<u64> {
        let inode_number = self.regular_file_inode(path)?;
        let inode = self.read_inode(inode_number)?;
        let lo = read_u32(&inode, I_SIZE_LO_OFFSET)? as u64;
        let hi = read_u32(&inode, I_SIZE_HI_OFFSET)? as u64;
        Ok(lo | (hi << 32))
    }

    /// The physical byte offset of the file's inode record on the volume,
    /// for the same-size `i_size` update that truncates the rewritten file.
    pub fn inode_source_offset(&self, path: &str) -> io::Result<u64> {
        let inode_number = self.regular_file_inode(path)?;
        if inode_number < 1 {
            return Err(invalid_fs_data("inode number must be >= 1"));
        }
        let group = (inode_number - 1) / self.inodes_per_group;
        let local_index = (inode_number - 1) % self.inodes_per_group;
        if group >= self.num_block_groups {
            return Err(invalid_fs_data(
                "inode belongs to a non-existent block group",
            ));
        }
        let descriptor = self.read_bg_descriptor(group)?;
        let table_block = inode_table_block_from_descriptor(&descriptor, self.has_64bit)?;
        let byte_offset = local_index as u64 * self.inode_size as u64;
        let table_offset = self.block_to_offset(table_block)?;
        table_offset
            .checked_add(byte_offset)
            .ok_or_else(|| invalid_fs_data("inode record offset overflows"))
    }

    fn regular_file_inode(&self, path: &str) -> io::Result<u32> {
        match self.resolve_path(path)? {
            Some((inode_number, false)) => Ok(inode_number),
            Some((_, true)) => Err(invalid_fs_data(format!("{path} is a directory"))),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{path} not found"),
            )),
        }
    }

    fn collect_extents(
        &self,
        node_data: &[u8],
        output: &mut Vec<Ext4FileExtent>,
    ) -> io::Result<()> {
        let header = Ext4ExtentHeader::parse(node_data)?;
        if header.eh_depth == 0 {
            for extent in parse_leaf_extents(node_data, header.eh_entries)? {
                if extent.is_unwritten() {
                    return Err(invalid_fs_data(
                        "file has unwritten extents; refusing to edit in place",
                    ));
                }
                let start_block = ((extent.ee_start_hi as u64) << 32) | extent.ee_start_lo as u64;
                output.push(Ext4FileExtent {
                    logical_offset: u64::from(extent.ee_block)
                        .checked_mul(self.block_size)
                        .ok_or_else(|| invalid_fs_data("ext4 extent logical offset overflows"))?,
                    volume_offset: self.block_to_offset(start_block)?,
                    length: u64::from(extent.block_count())
                        .checked_mul(self.block_size)
                        .ok_or_else(|| invalid_fs_data("ext4 extent length overflows"))?,
                });
            }
            return Ok(());
        }
        for child_block in index_child_blocks(node_data, header.eh_entries)? {
            let child_data = self.read_block(child_block)?;
            self.collect_extents(&child_data, output)?;
        }
        Ok(())
    }
}

fn parse_leaf_extents(data: &[u8], entries: u16) -> io::Result<Vec<Ext4Extent>> {
    let mut extents = Vec::new();
    for index in 0..entries as usize {
        let offset = 12 + index * 12;
        if offset + 12 > data.len() {
            break;
        }
        extents.push(Ext4Extent::parse(&data[offset..offset + 12])?);
    }
    Ok(extents)
}

fn index_child_blocks(data: &[u8], entries: u16) -> io::Result<Vec<u64>> {
    let mut blocks = Vec::new();
    for index in 0..entries as usize {
        let offset = 12 + index * 12;
        if offset + 12 > data.len() {
            break;
        }
        let low = u32::from_le_bytes(
            data[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| invalid_fs_data("extent index block parse error"))?,
        ) as u64;
        let high = u16::from_le_bytes([data[offset + 8], data[offset + 9]]) as u64;
        blocks.push(low | (high << 32));
    }
    Ok(blocks)
}

fn read_u32(data: &[u8], offset: usize) -> io::Result<u32> {
    data.get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| invalid_fs_data("inode field out of bounds"))
}
