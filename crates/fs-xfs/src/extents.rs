use crate::{
    be_u16, be_u32, be_u64, di_off, XfsExtent, XfsReader, BMA3_MAGIC, BMAP_MAGIC,
    BMBT_BLOCK_HDR_SIZE, BMBT_CRC_BLOCK_HDR_SIZE, BMBT_REC_SIZE, BMBT_SHORT_ROOT_HDR_SIZE,
    FORMAT_BTREE, FORMAT_EXTENTS, FORMAT_LOCAL,
};
use evidence_core::filesystem::{
    fs_out_of_memory, invalid_fs_data, truncate_data_to_declared_size,
};
use std::io;

impl XfsReader {
    fn read_extent_data(&self, inode: &[u8], file_size: u64) -> io::Result<Vec<u8>> {
        let extents = Self::inline_extents(inode)?;
        self.read_extents_data(&extents, file_size)
    }

    fn read_extent_data_range(
        &self,
        inode: &[u8],
        file_size: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        if length == 0 || offset >= file_size {
            return Ok(Vec::new());
        }
        let range_end = offset.saturating_add(length as u64).min(file_size);
        let capacity = usize::try_from(range_end.saturating_sub(offset))
            .map_err(|_| fs_out_of_memory("xfs range exceeds addressable memory"))?;
        let mut data = Vec::with_capacity(capacity);
        let mut next_offset = offset;
        let extents = Self::inline_extents(inode)?;
        self.read_extents_data_range(&extents, offset, range_end, &mut next_offset, &mut data)?;
        Ok(data)
    }

    fn read_extents_data(&self, extents: &[XfsExtent], file_size: u64) -> io::Result<Vec<u8>> {
        let capacity = usize::try_from(file_size)
            .map_err(|_| fs_out_of_memory("xfs file exceeds addressable memory"))?;
        let mut data = Vec::with_capacity(capacity);
        let mut next_offset = 0;
        self.read_extents_data_range(extents, 0, file_size, &mut next_offset, &mut data)?;
        Ok(truncate_data_to_declared_size(data, file_size))
    }

    fn read_extents_data_range(
        &self,
        extents: &[XfsExtent],
        range_start: u64,
        range_end: u64,
        next_offset: &mut u64,
        data: &mut Vec<u8>,
    ) -> io::Result<()> {
        let mut extents = extents.to_vec();
        extents.sort_by_key(|extent| extent.logical);
        for extent in extents {
            self.read_extent_range(extent, range_start, range_end, next_offset, data)?;
        }
        append_zeroes(data, range_end.saturating_sub(*next_offset))?;
        *next_offset = range_end;
        Ok(())
    }

    fn read_btree_data(&self, inode: &[u8], file_size: u64) -> io::Result<Vec<u8>> {
        let extents = self.collect_btree_extents(inode)?;
        self.read_extents_data(&extents, file_size)
    }

    pub(crate) fn collect_btree_extents(&self, inode: &[u8]) -> io::Result<Vec<XfsExtent>> {
        let data_fork = Self::data_fork(inode)?;
        if data_fork.len() < BMBT_SHORT_ROOT_HDR_SIZE {
            return Ok(Vec::new());
        }

        let mut extents = Vec::new();
        if data_fork.len() >= 8 && be_u32(data_fork, 0) == BMAP_MAGIC {
            self.walk_btree_node_extents(data_fork, true, &mut extents)?;
        } else {
            self.walk_bmdr_root_extents(data_fork, &mut extents)?;
        }
        Ok(extents)
    }

    fn read_btree_data_range(
        &self,
        inode: &[u8],
        file_size: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        if length == 0 || offset >= file_size {
            return Ok(Vec::new());
        }
        let range_end = offset.saturating_add(length as u64).min(file_size);
        let capacity = usize::try_from(range_end.saturating_sub(offset))
            .map_err(|_| fs_out_of_memory("xfs btree range exceeds addressable memory"))?;
        let mut data = Vec::with_capacity(capacity);
        let mut next_offset = offset;
        let extents = self.collect_btree_extents(inode)?;
        self.read_extents_data_range(&extents, offset, range_end, &mut next_offset, &mut data)?;
        Ok(data)
    }

    fn walk_bmdr_root_extents(&self, node: &[u8], extents: &mut Vec<XfsExtent>) -> io::Result<()> {
        if node.len() < BMBT_SHORT_ROOT_HDR_SIZE {
            return Ok(());
        }
        let level = be_u16(node, 0);
        let numrecs = usize::from(be_u16(node, 2));
        if numrecs == 0 {
            return Ok(());
        }

        if level == 0 {
            for index in 0..numrecs {
                let offset = BMBT_SHORT_ROOT_HDR_SIZE + index * BMBT_REC_SIZE;
                if offset + BMBT_REC_SIZE > node.len() {
                    break;
                }
                extents.push(Self::decode_extent(&node[offset..offset + BMBT_REC_SIZE]));
            }
            return Ok(());
        }

        let maxrecs = Self::bmdr_maxrecs(node.len(), false);
        let pointers_start = BMBT_SHORT_ROOT_HDR_SIZE + maxrecs * 8;
        for index in 0..numrecs {
            let offset = pointers_start + index * 8;
            if offset + 8 > node.len() {
                break;
            }
            let child_ptr = be_u64(node, offset);
            let child_block = self.read_block(child_ptr)?;
            self.walk_btree_child_extents(child_ptr, &child_block, extents)?;
        }
        Ok(())
    }

    fn walk_btree_child_extents(
        &self,
        fsb: u64,
        node: &[u8],
        extents: &mut Vec<XfsExtent>,
    ) -> io::Result<()> {
        let (header_size, level, numrecs) = Self::parse_btree_block_header(node, fsb)?;
        if level == 0 {
            for index in 0..numrecs {
                let offset = header_size + index * BMBT_REC_SIZE;
                if offset + BMBT_REC_SIZE > node.len() {
                    break;
                }
                extents.push(Self::decode_extent(&node[offset..offset + BMBT_REC_SIZE]));
            }
            return Ok(());
        }

        let maxrecs = Self::bmbt_block_maxrecs(node.len(), header_size, false);
        let pointers_start = header_size + maxrecs * 8;
        for index in 0..numrecs {
            let offset = pointers_start + index * 8;
            if offset + 8 > node.len() {
                break;
            }
            let child_ptr = be_u64(node, offset);
            let child_block = self.read_block(child_ptr)?;
            self.walk_btree_child_extents(child_ptr, &child_block, extents)?;
        }
        Ok(())
    }

    fn bmdr_maxrecs(block_len: usize, leaf: bool) -> usize {
        let data_len = block_len.saturating_sub(BMBT_SHORT_ROOT_HDR_SIZE);
        if leaf {
            data_len / BMBT_REC_SIZE
        } else {
            data_len / 16
        }
    }

    fn bmbt_block_maxrecs(block_len: usize, header_size: usize, leaf: bool) -> usize {
        let data_len = block_len.saturating_sub(header_size);
        if leaf {
            data_len / BMBT_REC_SIZE
        } else {
            data_len / 16
        }
    }

    fn parse_btree_block_header(node: &[u8], fsb: u64) -> io::Result<(usize, u16, usize)> {
        if node.len() < 8 {
            return Err(invalid_fs_data(format!(
                "bmbt child block at FSB {fsb} too short for magic ({} bytes)",
                node.len()
            )));
        }
        let magic = be_u32(node, 0);
        let header_size = match magic {
            BMAP_MAGIC => BMBT_BLOCK_HDR_SIZE,
            BMA3_MAGIC => BMBT_CRC_BLOCK_HDR_SIZE,
            _ => {
                return Err(invalid_fs_data(format!(
                    "invalid bmbt child block magic 0x{magic:08X} at FSB {fsb}"
                )))
            }
        };
        if node.len() < header_size {
            return Err(invalid_fs_data(format!(
                "bmbt child block at FSB {fsb} with magic 0x{magic:08X} too short ({} < {header_size})",
                node.len()
            )));
        }
        Ok((header_size, be_u16(node, 4), usize::from(be_u16(node, 6))))
    }

    fn walk_btree_node_extents(
        &self,
        node: &[u8],
        is_inode_root: bool,
        extents: &mut Vec<XfsExtent>,
    ) -> io::Result<()> {
        let header_size = if is_inode_root { 8 } else { 24 };
        if node.len() < header_size {
            return Ok(());
        }
        let level = be_u16(node, 4);
        let numrecs = usize::from(be_u16(node, 6));
        if level == 0 {
            const LEAF_SLOT: usize = 24;
            for index in 0..numrecs {
                let offset = header_size + index * LEAF_SLOT;
                if offset + LEAF_SLOT > node.len() {
                    break;
                }
                extents.push(Self::decode_extent(&node[offset + 8..offset + 24]));
            }
            return Ok(());
        }

        const INTERNAL_SLOT: usize = 16;
        for index in 0..numrecs {
            let offset = header_size + index * INTERNAL_SLOT;
            if offset + INTERNAL_SLOT > node.len() {
                break;
            }
            let child_ptr = be_u64(node, offset + 8);
            let child_block = self.read_block(child_ptr)?;
            self.walk_btree_child_extents(child_ptr, &child_block, extents)?;
        }
        Ok(())
    }

    fn read_extent_range(
        &self,
        extent: XfsExtent,
        range_start: u64,
        range_end: u64,
        next_offset: &mut u64,
        data: &mut Vec<u8>,
    ) -> io::Result<()> {
        let extent_start = extent.logical.checked_mul(self.block_size).ok_or_else(|| {
            invalid_fs_data(format!(
                "extent logical block {} byte offset overflows (block_size={})",
                extent.logical, self.block_size
            ))
        })?;
        let extent_len = extent
            .block_count
            .checked_mul(self.block_size)
            .ok_or_else(|| {
                invalid_fs_data(format!(
                    "extent length {} blocks overflows (block_size={})",
                    extent.block_count, self.block_size
                ))
            })?;
        let extent_end = extent_start.checked_add(extent_len).ok_or_else(|| {
            invalid_fs_data(format!(
                "extent logical range overflows (logical={} blocks={})",
                extent.logical, extent.block_count
            ))
        })?;
        let overlap_start = extent_start.max(range_start).max(*next_offset);
        let overlap_end = extent_end.min(range_end);
        if overlap_start >= overlap_end {
            return Ok(());
        }
        if *next_offset < overlap_start {
            append_zeroes(data, overlap_start - *next_offset)?;
        }
        let read_len = usize::try_from(overlap_end - overlap_start)
            .map_err(|_| fs_out_of_memory("xfs extent range exceeds addressable memory"))?;
        if extent.unwritten {
            append_zeroes(data, read_len as u64)?;
        } else {
            let physical_offset = self
                .fsblock_to_offset(extent.start_block)?
                .checked_add(overlap_start - extent_start)
                .ok_or_else(|| {
                    invalid_fs_data(format!(
                        "extent physical offset overflows (start_fsb={} logical={})",
                        extent.start_block, extent.logical
                    ))
                })?;
            let chunk = self.read_bytes_at(physical_offset, read_len).map_err(|error| {
                if error.kind() == io::ErrorKind::UnexpectedEof {
                    invalid_fs_data(format!(
                        "allocated XFS extent read truncated at physical offset {physical_offset} length {read_len} (start_fsb={} logical={}): {error}",
                        extent.start_block, extent.logical
                    ))
                } else {
                    error
                }
            })?;
            data.extend_from_slice(&chunk);
        }
        *next_offset = overlap_end;
        Ok(())
    }

    pub(crate) fn read_file_content(&self, ino: u64) -> io::Result<Vec<u8>> {
        let inode = self.read_inode(ino)?;
        Self::validate_inode_magic(&inode)?;
        let format = inode[di_off::FORMAT];
        let size = be_u64(&inode, di_off::SIZE);
        match format {
            FORMAT_LOCAL => {
                let data_fork = Self::data_fork(&inode)?;
                Ok(truncate_data_to_declared_size(data_fork.to_vec(), size))
            }
            FORMAT_EXTENTS => self.read_extent_data(&inode, size),
            FORMAT_BTREE => self.read_btree_data(&inode, size),
            other => Err(invalid_fs_data(format!("unsupported di_format {other}"))),
        }
    }

    pub(crate) fn read_file_content_range(
        &self,
        ino: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        let inode = self.read_inode(ino)?;
        Self::validate_inode_magic(&inode)?;
        let format = inode[di_off::FORMAT];
        let size = be_u64(&inode, di_off::SIZE);
        if length == 0 || offset >= size {
            return Ok(Vec::new());
        }
        match format {
            FORMAT_LOCAL => {
                let data_fork = Self::data_fork(&inode)?;
                let start = usize::try_from(offset)
                    .ok()
                    .map(|value| value.min(data_fork.len()))
                    .unwrap_or(data_fork.len());
                let declared_end = usize::try_from(size)
                    .ok()
                    .map(|value| value.min(data_fork.len()))
                    .unwrap_or(data_fork.len());
                let end = start.saturating_add(length).min(declared_end);
                Ok(data_fork[start..end].to_vec())
            }
            FORMAT_EXTENTS => self.read_extent_data_range(&inode, size, offset, length),
            FORMAT_BTREE => self.read_btree_data_range(&inode, size, offset, length),
            other => Err(invalid_fs_data(format!("unsupported di_format {other}"))),
        }
    }
}

fn append_zeroes(data: &mut Vec<u8>, count: u64) -> io::Result<()> {
    let count = usize::try_from(count)
        .map_err(|_| fs_out_of_memory("xfs sparse range exceeds addressable memory"))?;
    let new_len = data
        .len()
        .checked_add(count)
        .ok_or_else(|| fs_out_of_memory("xfs sparse range exceeds addressable memory"))?;
    data.resize(new_len, 0);
    Ok(())
}
