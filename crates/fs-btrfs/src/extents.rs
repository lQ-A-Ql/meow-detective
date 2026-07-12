use crate::format::{EXTENT_DATA_KEY, EXTENT_INLINE};
use crate::types::BtrfsKey;
use crate::BtrfsReader;
use evidence_core::filesystem::{
    fs_out_of_memory, invalid_fs_data, truncate_data_to_declared_size,
};
use std::io::{self, Read, Seek, SeekFrom};

impl BtrfsReader {
    pub(crate) fn read_file_extents(
        &self,
        tree_root_bytenr: u64,
        inode_objectid: u64,
        declared_size: u64,
    ) -> io::Result<Vec<u8>> {
        let lower_bound = extent_key(inode_objectid, 0);
        let upper_bound = extent_key(inode_objectid, u64::MAX);
        let mut data = Vec::new();

        for (leaf_data, items) in
            self.collect_candidate_leaves(tree_root_bytenr, &lower_bound, &upper_bound)?
        {
            for index in
                Self::find_items_by_object_and_type(&items, inode_objectid, EXTENT_DATA_KEY)
            {
                let item_data = Self::get_item_data(&leaf_data, &items[index]);
                if item_data.len() < 21 {
                    continue;
                }
                if item_data[20] == EXTENT_INLINE {
                    data.extend_from_slice(&item_data[21..]);
                    continue;
                }
                let Some((disk_bytenr, _, num_bytes)) = parse_regular_extent(item_data)? else {
                    continue;
                };
                let mut buf = vec![0u8; num_bytes as usize];
                let mut reader = self.reader.borrow_mut();
                reader.seek(SeekFrom::Start(self.volume_offset + disk_bytenr))?;
                reader.read_exact(&mut buf)?;
                data.extend_from_slice(&buf);
            }
        }
        Ok(truncate_data_to_declared_size(data, declared_size))
    }

    pub(crate) fn read_file_extents_range(
        &self,
        tree_root_bytenr: u64,
        inode_objectid: u64,
        declared_size: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        if length == 0 || offset >= declared_size {
            return Ok(Vec::new());
        }

        let range_end = offset.saturating_add(length as u64).min(declared_size);
        let capacity = usize::try_from(range_end.saturating_sub(offset))
            .map_err(|_| fs_out_of_memory("btrfs range exceeds addressable memory"))?;
        let mut data = Vec::with_capacity(capacity);
        let mut next_offset = offset;
        let lower_bound = extent_key(inode_objectid, 0);
        let upper_bound = extent_key(inode_objectid, range_end);

        for (leaf_data, items) in
            self.collect_candidate_leaves(tree_root_bytenr, &lower_bound, &upper_bound)?
        {
            for index in
                Self::find_items_by_object_and_type(&items, inode_objectid, EXTENT_DATA_KEY)
            {
                let item = &items[index];
                let item_data = Self::get_item_data(&leaf_data, item);
                if item_data.len() < 21 {
                    continue;
                }
                if item_data[20] == EXTENT_INLINE {
                    append_inline_overlap(
                        &mut data,
                        &mut next_offset,
                        item.key.offset,
                        &item_data[21..],
                        offset,
                        range_end,
                    )?;
                    continue;
                }
                let Some((disk_bytenr, extent_offset, num_bytes)) =
                    parse_regular_extent(item_data)?
                else {
                    continue;
                };
                self.append_regular_overlap(
                    &mut data,
                    &mut next_offset,
                    item.key.offset,
                    disk_bytenr,
                    extent_offset,
                    num_bytes,
                    offset,
                    range_end,
                )?;
            }
        }

        append_zeroes(&mut data, range_end.saturating_sub(next_offset))?;
        Ok(data)
    }

    #[allow(clippy::too_many_arguments)]
    fn append_regular_overlap(
        &self,
        data: &mut Vec<u8>,
        next_offset: &mut u64,
        extent_start: u64,
        disk_bytenr: u64,
        extent_offset: u64,
        num_bytes: u64,
        range_start: u64,
        range_end: u64,
    ) -> io::Result<()> {
        let extent_end = extent_start.saturating_add(num_bytes);
        let overlap_start = extent_start.max(range_start);
        let overlap_end = extent_end.min(range_end);
        if overlap_start >= overlap_end {
            return Ok(());
        }
        if *next_offset < overlap_start {
            append_zeroes(data, overlap_start - *next_offset)?;
        }
        let logical = disk_bytenr + extent_offset + overlap_start.saturating_sub(extent_start);
        let read_len = usize::try_from(overlap_end - overlap_start)
            .map_err(|_| fs_out_of_memory("btrfs extent range exceeds addressable memory"))?;
        data.extend_from_slice(&self.read_logical_range(logical, read_len)?);
        *next_offset = overlap_end;
        Ok(())
    }
}

fn extent_key(objectid: u64, offset: u64) -> BtrfsKey {
    BtrfsKey {
        objectid,
        ty: EXTENT_DATA_KEY,
        offset,
    }
}

fn parse_regular_extent(data: &[u8]) -> io::Result<Option<(u64, u64, u64)>> {
    if data.len() < 53 {
        return Ok(None);
    }
    let disk_bytenr = u64::from_le_bytes(
        data[21..29]
            .try_into()
            .map_err(|_| invalid_fs_data("disk parse error"))?,
    );
    let extent_offset = u64::from_le_bytes(
        data[37..45]
            .try_into()
            .map_err(|_| invalid_fs_data("disk parse error"))?,
    );
    let num_bytes = u64::from_le_bytes(
        data[45..53]
            .try_into()
            .map_err(|_| invalid_fs_data("disk parse error"))?,
    );
    Ok(Some((disk_bytenr, extent_offset, num_bytes)))
}

fn append_inline_overlap(
    data: &mut Vec<u8>,
    next_offset: &mut u64,
    extent_start: u64,
    inline_data: &[u8],
    range_start: u64,
    range_end: u64,
) -> io::Result<()> {
    let extent_end = extent_start.saturating_add(inline_data.len() as u64);
    let overlap_start = extent_start.max(range_start);
    let overlap_end = extent_end.min(range_end);
    if overlap_start >= overlap_end {
        return Ok(());
    }
    if *next_offset < overlap_start {
        append_zeroes(data, overlap_start - *next_offset)?;
    }
    let start = usize::try_from(overlap_start - extent_start)
        .map_err(|_| fs_out_of_memory("btrfs inline range exceeds addressable memory"))?;
    let end = usize::try_from(overlap_end - extent_start)
        .map_err(|_| fs_out_of_memory("btrfs inline range exceeds addressable memory"))?;
    data.extend_from_slice(&inline_data[start..end]);
    *next_offset = overlap_end;
    Ok(())
}

pub(crate) fn append_zeroes(data: &mut Vec<u8>, count: u64) -> io::Result<()> {
    let count = usize::try_from(count)
        .map_err(|_| fs_out_of_memory("btrfs sparse range exceeds addressable memory"))?;
    let new_len = data
        .len()
        .checked_add(count)
        .ok_or_else(|| fs_out_of_memory("btrfs sparse range exceeds addressable memory"))?;
    data.resize(new_len, 0);
    Ok(())
}
