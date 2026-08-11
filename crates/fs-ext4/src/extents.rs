use crate::format::{Ext4Extent, Ext4ExtentHeader};
use crate::Ext4Reader;
use evidence_core::filesystem::{fs_out_of_memory, invalid_fs_data};
use std::io;

impl Ext4Reader {
    pub(crate) fn read_extent_data(&self, i_block: &[u8], file_size: u64) -> io::Result<Vec<u8>> {
        self.validate_declared_size(file_size)?;
        if i_block.len() < 12 {
            return Ok(Vec::new());
        }
        let header = Ext4ExtentHeader::parse(i_block)?;
        if header.eh_depth == 0 {
            self.read_extent_leaves(i_block, file_size, 0)
        } else {
            self.walk_extent_tree(i_block, file_size, 0, header.eh_depth)
        }
    }

    pub(crate) fn read_extent_data_range(
        &self,
        i_block: &[u8],
        file_size: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        self.validate_declared_size(file_size)?;
        if i_block.len() < 12 || length == 0 || offset >= file_size {
            return Ok(Vec::new());
        }
        let range_end = offset.saturating_add(length as u64).min(file_size);
        let capacity = usize::try_from(range_end.saturating_sub(offset))
            .map_err(|_| fs_out_of_memory("ext4 range exceeds addressable memory"))?;
        let mut data = Vec::with_capacity(capacity);
        let mut next_offset = offset;
        let header = Ext4ExtentHeader::parse(i_block)?;
        if header.eh_depth == 0 {
            self.read_extent_leaves_range(i_block, offset, range_end, &mut next_offset, &mut data)?;
        } else {
            self.walk_extent_tree_range(
                i_block,
                header.eh_depth,
                offset,
                range_end,
                &mut next_offset,
                &mut data,
            )?;
        }
        append_zeroes(&mut data, range_end.saturating_sub(next_offset))?;
        Ok(data)
    }

    /// Reads leaf extents, accumulating at most `file_size - collected` bytes
    /// so a crafted extent list cannot inflate memory beyond the declared
    /// inode size. Extents starting at or past `file_size` hold no file data
    /// and are skipped.
    fn read_extent_leaves(
        &self,
        node_data: &[u8],
        file_size: u64,
        collected: u64,
    ) -> io::Result<Vec<u8>> {
        let header = Ext4ExtentHeader::parse(node_data)?;
        let mut data = Vec::new();
        for extent in parse_extents(node_data, header.eh_entries)? {
            let gathered = collected.saturating_add(data.len() as u64);
            if gathered >= file_size {
                break;
            }
            let logical_start = u64::from(extent.ee_block).saturating_mul(self.block_size);
            if logical_start >= file_size {
                continue;
            }
            let remaining = file_size - gathered;
            let start_block = ((extent.ee_start_hi as u64) << 32) | extent.ee_start_lo as u64;
            let block_count = u64::from(extent.block_count());
            if extent.is_unwritten() {
                let zeroes = block_count.saturating_mul(self.block_size).min(remaining);
                append_zeroes(&mut data, zeroes)?;
            } else {
                self.append_extent_blocks(&mut data, start_block, block_count, remaining)?;
            }
        }
        Ok(data)
    }

    fn append_extent_blocks(
        &self,
        data: &mut Vec<u8>,
        start_block: u64,
        block_count: u64,
        budget: u64,
    ) -> io::Result<()> {
        let mut remaining = budget;
        for block in start_block..start_block.saturating_add(block_count) {
            if remaining == 0 {
                break;
            }
            let block_data = self.read_block(block)?;
            let take = (block_data.len() as u64).min(remaining);
            data.extend_from_slice(&block_data[..take as usize]);
            remaining -= take;
        }
        Ok(())
    }

    fn read_extent_leaves_range(
        &self,
        node_data: &[u8],
        range_start: u64,
        range_end: u64,
        next_offset: &mut u64,
        data: &mut Vec<u8>,
    ) -> io::Result<()> {
        let header = Ext4ExtentHeader::parse(node_data)?;
        for extent in parse_extents(node_data, header.eh_entries)? {
            self.read_extent_range(extent, range_start, range_end, next_offset, data)?;
        }
        Ok(())
    }

    fn read_extent_range(
        &self,
        extent: Ext4Extent,
        range_start: u64,
        range_end: u64,
        next_offset: &mut u64,
        data: &mut Vec<u8>,
    ) -> io::Result<()> {
        let extent_start = extent.ee_block as u64 * self.block_size;
        let extent_end = extent_start.saturating_add(extent.block_count() as u64 * self.block_size);
        let overlap_start = extent_start.max(range_start);
        let overlap_end = extent_end.min(range_end);
        if overlap_start >= overlap_end {
            return Ok(());
        }
        if *next_offset < overlap_start {
            append_zeroes(data, overlap_start - *next_offset)?;
        }
        let read_len = usize::try_from(overlap_end - overlap_start)
            .map_err(|_| fs_out_of_memory("ext4 extent range exceeds addressable memory"))?;
        if extent.is_unwritten() {
            append_zeroes(data, read_len as u64)?;
        } else {
            let start_block = ((extent.ee_start_hi as u64) << 32) | extent.ee_start_lo as u64;
            let physical_offset = self
                .block_to_offset(start_block)?
                .checked_add(overlap_start.saturating_sub(extent_start))
                .ok_or_else(|| invalid_fs_data("ext4 extent byte offset overflows"))?;
            data.extend_from_slice(&self.read_bytes_at(physical_offset, read_len)?);
        }
        *next_offset = overlap_end;
        Ok(())
    }

    fn walk_extent_tree(
        &self,
        node_data: &[u8],
        file_size: u64,
        collected: u64,
        depth: u16,
    ) -> io::Result<Vec<u8>> {
        let header = Ext4ExtentHeader::parse(node_data)?;
        let mut data = Vec::new();
        for child_block in parse_index_blocks(node_data, header.eh_entries)? {
            let gathered = collected.saturating_add(data.len() as u64);
            if gathered >= file_size {
                break;
            }
            let child_data = self.read_block(child_block)?;
            let mut chunk = if depth == 1 {
                self.read_extent_leaves(&child_data, file_size, gathered)?
            } else {
                self.walk_extent_tree(&child_data, file_size, gathered, depth - 1)?
            };
            data.append(&mut chunk);
        }
        Ok(data)
    }

    fn walk_extent_tree_range(
        &self,
        node_data: &[u8],
        depth: u16,
        range_start: u64,
        range_end: u64,
        next_offset: &mut u64,
        data: &mut Vec<u8>,
    ) -> io::Result<()> {
        let header = Ext4ExtentHeader::parse(node_data)?;
        for child_block in parse_index_blocks(node_data, header.eh_entries)? {
            let child_data = self.read_block(child_block)?;
            if depth == 1 {
                self.read_extent_leaves_range(
                    &child_data,
                    range_start,
                    range_end,
                    next_offset,
                    data,
                )?;
            } else {
                self.walk_extent_tree_range(
                    &child_data,
                    depth - 1,
                    range_start,
                    range_end,
                    next_offset,
                    data,
                )?;
            }
        }
        Ok(())
    }
}

fn parse_extents(data: &[u8], entries: u16) -> io::Result<Vec<Ext4Extent>> {
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

fn parse_index_blocks(data: &[u8], entries: u16) -> io::Result<Vec<u64>> {
    let mut blocks = Vec::new();
    for index in 0..entries as usize {
        let offset = 12 + index * 12;
        if offset + 12 > data.len() {
            break;
        }
        let low = u32::from_le_bytes(
            data[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        ) as u64;
        let high = u16::from_le_bytes([data[offset + 8], data[offset + 9]]) as u64;
        blocks.push(low | (high << 32));
    }
    Ok(blocks)
}

fn append_zeroes(data: &mut Vec<u8>, count: u64) -> io::Result<()> {
    let count = usize::try_from(count)
        .map_err(|_| fs_out_of_memory("ext4 sparse range exceeds addressable memory"))?;
    let new_len = data
        .len()
        .checked_add(count)
        .ok_or_else(|| fs_out_of_memory("ext4 sparse range exceeds addressable memory"))?;
    data.resize(new_len, 0);
    Ok(())
}
