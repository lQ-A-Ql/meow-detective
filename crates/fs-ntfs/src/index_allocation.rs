use std::io;

use crate::attribute::DataAttributeExtent;
use crate::directory::{parse_indx_entries, DirEntry};
use crate::utils::apply_record_fixup;
use crate::{invalid_fs_data, ATTR_TYPE_BITMAP, ATTR_TYPE_INDEX_ALLOCATION};

const INDEX_VBN_BLOCK_BYTES: u64 = 512;

impl crate::NtfsReader {
    pub(crate) fn index_allocation_entries(
        &self,
        inode: u64,
        record: &[u8],
    ) -> io::Result<Vec<DirEntry>> {
        let allocation = self.collect_index_extents(inode, record, ATTR_TYPE_INDEX_ALLOCATION)?;
        if allocation.is_empty() {
            return Ok(Vec::new());
        }
        let bitmap = self.collect_index_extents(inode, record, ATTR_TYPE_BITMAP)?;
        if bitmap.is_empty() {
            return Err(invalid_fs_data(
                "NTFS $INDEX_ALLOCATION has no matching $BITMAP stream",
            ));
        }
        let allocation_data = self.read_data_extents_to_vec(&allocation)?;
        let bitmap_data = self.read_data_extents_to_vec(&bitmap)?;
        parse_index_allocation(
            &allocation_data,
            &bitmap_data,
            self.index_record_size as usize,
            self.bytes_per_sector as usize,
            self.cluster_size,
        )
    }

    fn collect_index_extents(
        &self,
        inode: u64,
        record: &[u8],
        attribute_type: u32,
    ) -> io::Result<Vec<DataAttributeExtent>> {
        let named =
            self.collect_attribute_extents_from_base(inode, record, attribute_type, Some("$I30"))?;
        if !named.is_empty() {
            return Ok(named);
        }
        self.collect_attribute_extents_from_base(inode, record, attribute_type, None)
    }
}

fn parse_index_allocation(
    allocation: &[u8],
    bitmap: &[u8],
    record_size: usize,
    sector_size: usize,
    cluster_size: u64,
) -> io::Result<Vec<DirEntry>> {
    if record_size < 0x30 || sector_size < 2 || !record_size.is_multiple_of(sector_size) {
        return Err(invalid_fs_data("invalid NTFS index-record geometry"));
    }
    if !allocation.len().is_multiple_of(record_size) {
        return Err(invalid_fs_data(
            "NTFS index-allocation length is not record aligned",
        ));
    }
    let record_count = allocation.len() / record_size;
    let mut entries = Vec::new();
    for bit in set_bitmap_bits(bitmap) {
        if bit >= record_count {
            return Err(invalid_fs_data(format!(
                "NTFS index bitmap selects missing record {bit}"
            )));
        }
        let start = bit
            .checked_mul(record_size)
            .ok_or_else(|| invalid_fs_data("NTFS index-record offset overflow"))?;
        let expected_vbn = bitmap_bit_vbn(bit, record_size, cluster_size)?;
        entries.extend(parse_index_record(
            &allocation[start..start + record_size],
            sector_size,
            expected_vbn,
        )?);
    }
    Ok(entries)
}

fn parse_index_record(
    record: &[u8],
    sector_size: usize,
    expected_vbn: u64,
) -> io::Result<Vec<DirEntry>> {
    if record.len() < 0x30 || &record[0..4] != b"INDX" {
        return Err(invalid_fs_data(
            "allocated NTFS index record has invalid magic",
        ));
    }
    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    let expected_usa_count = record.len() / sector_size + 1;
    if usa_offset < 0x18
        || usa_count != expected_usa_count
        || usa_offset
            .checked_add(usa_count.saturating_mul(2))
            .is_none_or(|end| end > record.len())
    {
        return Err(invalid_fs_data("invalid NTFS index-record update sequence"));
    }

    let actual_vbn = u64::from_le_bytes(record[0x10..0x18].try_into().unwrap_or([0; 8]));
    if actual_vbn != expected_vbn {
        return Err(invalid_fs_data(format!(
            "NTFS index-record VBN mismatch: expected {expected_vbn}, found {actual_vbn}"
        )));
    }
    let mut fixed = record.to_vec();
    apply_record_fixup(&mut fixed, sector_size)?;
    parse_fixed_index_record(&fixed)
}

fn parse_fixed_index_record(record: &[u8]) -> io::Result<Vec<DirEntry>> {
    let header = 0x18usize;
    let entries_offset = read_u32(record, header)? as usize;
    let entries_size = read_u32(record, header + 4)? as usize;
    let allocated_size = read_u32(record, header + 8)? as usize;
    let start = header
        .checked_add(entries_offset)
        .ok_or_else(|| invalid_fs_data("NTFS index entry offset overflow"))?;
    let end = header
        .checked_add(entries_size)
        .ok_or_else(|| invalid_fs_data("NTFS index entry length overflow"))?;
    let allocated_end = header
        .checked_add(allocated_size)
        .ok_or_else(|| invalid_fs_data("NTFS index allocation length overflow"))?;
    if start > end || end > allocated_end || allocated_end > record.len() {
        return Err(invalid_fs_data("invalid NTFS index entry range"));
    }
    Ok(parse_indx_entries(&record[start..end]))
}

fn set_bitmap_bits(bitmap: &[u8]) -> impl Iterator<Item = usize> + '_ {
    bitmap.iter().enumerate().flat_map(|(byte_index, byte)| {
        (0..8).filter_map(move |bit| (byte & (1 << bit) != 0).then_some(byte_index * 8 + bit))
    })
}

fn bitmap_bit_vbn(bit: usize, record_size: usize, cluster_size: u64) -> io::Result<u64> {
    let record_size = u64::try_from(record_size)
        .map_err(|_| invalid_fs_data("NTFS index-record size overflow"))?;
    let vbn_unit = if record_size < cluster_size {
        INDEX_VBN_BLOCK_BYTES
    } else {
        cluster_size
    };
    if vbn_unit == 0 || !record_size.is_multiple_of(vbn_unit) {
        return Err(invalid_fs_data(
            "NTFS index-record size is not aligned to its VBN unit",
        ));
    }
    let records_per_vbn = record_size / vbn_unit;
    u64::try_from(bit)
        .ok()
        .and_then(|value| value.checked_mul(records_per_vbn))
        .ok_or_else(|| invalid_fs_data("NTFS index VBN overflow"))
}

fn read_u32(data: &[u8], offset: usize) -> io::Result<u32> {
    data.get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| invalid_fs_data("truncated NTFS index header"))
}

#[cfg(test)]
#[path = "../tests/unit/index_allocation.rs"]
mod tests;
