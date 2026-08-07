//! Directory-index entry removal planning for the emulation overlay writer.
//!
//! Removing a directory entry (e.g. a leftover `OSDATA` node) is a namespace
//! edit: excise the entry from the parent directory's index and clear the
//! child record's in-use flag. This module analyses the live filesystem
//! read-only and produces an explicit list of disk edits; the caller applies
//! them through the COW overlay. Allocation sizes are never changed — the
//! index area keeps its allocation and only the used-size fields shrink, so
//! no attribute or record layout is disturbed.

use std::io;

use crate::attribute::DataAttributeExtent;
use crate::utils::validate_file_record;
use crate::{file_not_found, invalid_fs_data, ATTR_TYPE_INDEX_ALLOCATION, ATTR_TYPE_INDEX_ROOT};

/// One atomic byte-range replacement, expressed in the reader's coordinate
/// space (including any volume base the reader was opened with).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedDiskEdit {
    pub offset: u64,
    pub bytes: Vec<u8>,
}

/// The outcome of planning a directory-entry removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntryRemoval {
    pub edits: Vec<PlannedDiskEdit>,
    pub removed_inode: u64,
    pub was_directory: bool,
}

const INDEX_ENTRY_HAS_CHILD: u32 = 0x01;
const INDEX_ENTRY_TERMINATOR: u32 = 0x02;
const FILE_RECORD_IN_USE: u16 = 0x0001;

impl crate::NtfsReader {
    /// Plan the removal of `entry_name` from the directory at `dir_path`.
    pub fn plan_directory_entry_removal(
        &self,
        dir_path: &str,
        entry_name: &str,
    ) -> io::Result<DirectoryEntryRemoval> {
        let inode = self
            .resolve_path(dir_path)?
            .ok_or_else(|| file_not_found(dir_path))?;
        let record = self.read_mft_record(inode)?;
        validate_file_record(&record, inode)?;

        let Some(site) = self.locate_index_entry(inode, &record, entry_name)? else {
            return Err(file_not_found(&format!("{dir_path}/{entry_name}")));
        };
        let mut edits = Vec::new();
        let (removed_inode, was_directory) = match site {
            EntrySite::IndexRoot => {
                let mut edited = record.clone();
                let info = self.edit_index_root(&mut edited, entry_name)?;
                inverse_record_fixup(&mut edited, self.bytes_per_sector())?;
                edits.push(PlannedDiskEdit {
                    offset: self.mft_record_source_offset(inode)?,
                    bytes: edited,
                });
                info
            }
            EntrySite::IndexBlock { disk_offset } => {
                let mut block = vec![0u8; self.index_record_size as usize];
                self.read_at(disk_offset, &mut block)?;
                crate::utils::apply_record_fixup(&mut block, self.bytes_per_sector())?;
                let info = edit_index_block(&mut block, entry_name)?;
                inverse_record_fixup(&mut block, self.bytes_per_sector())?;
                edits.push(PlannedDiskEdit {
                    offset: disk_offset,
                    bytes: block,
                });
                info
            }
        };

        let mut child = self.read_mft_record(removed_inode)?;
        validate_file_record(&child, removed_inode)?;
        let flags = u16::from_le_bytes([child[0x16], child[0x17]]);
        if flags & FILE_RECORD_IN_USE != 0 {
            child[0x16] = (flags & !FILE_RECORD_IN_USE).to_le_bytes()[0];
            child[0x17] = (flags & !FILE_RECORD_IN_USE).to_le_bytes()[1];
            inverse_record_fixup(&mut child, self.bytes_per_sector())?;
            edits.push(PlannedDiskEdit {
                offset: self.mft_record_source_offset(removed_inode)?,
                bytes: child,
            });
        }
        Ok(DirectoryEntryRemoval {
            edits,
            removed_inode,
            was_directory,
        })
    }

    fn locate_index_entry(
        &self,
        inode: u64,
        record: &[u8],
        entry_name: &str,
    ) -> io::Result<Option<EntrySite>> {
        if let Some(root_pos) = find_index_root_attr(record) {
            let content = crate::attribute::resident_attr_content(
                record,
                root_pos,
                attr_len_at(record, root_pos)?,
            );
            if let Some(content) = content {
                if find_entry_span(&content[index_entries_region(content)?], entry_name)?.is_some()
                {
                    return Ok(Some(EntrySite::IndexRoot));
                }
                if index_root_has_allocation(content) {
                    if let Some(disk_offset) =
                        self.find_entry_in_allocation(inode, record, entry_name)?
                    {
                        return Ok(Some(EntrySite::IndexBlock { disk_offset }));
                    }
                }
            }
            return Ok(None);
        }
        Ok(None)
    }

    fn find_entry_in_allocation(
        &self,
        inode: u64,
        record: &[u8],
        entry_name: &str,
    ) -> io::Result<Option<u64>> {
        let extents = self.collect_attribute_extents_from_base(
            inode,
            record,
            ATTR_TYPE_INDEX_ALLOCATION,
            Some("$I30"),
        )?;
        let block_size = self.index_record_size as u64;
        for extent in &extents {
            let DataAttributeExtent::NonResident { runs, .. } = extent else {
                continue;
            };
            for run in runs {
                let run_bytes = run
                    .cluster_count
                    .checked_mul(self.cluster_size())
                    .ok_or_else(|| invalid_fs_data("index run length overflows"))?;
                for block_index in 0..run_bytes / block_size {
                    let disk_offset = self.data_run_source_offset(run)? + block_index * block_size;
                    let mut block = vec![0u8; block_size as usize];
                    self.read_at(disk_offset, &mut block)?;
                    if crate::utils::apply_record_fixup(&mut block, self.bytes_per_sector()).is_ok()
                        && block.starts_with(b"INDX")
                        && find_entry_span(&block[index_block_entries_region(&block)?], entry_name)?
                            .is_some()
                    {
                        return Ok(Some(disk_offset));
                    }
                }
            }
        }
        Ok(None)
    }

    fn edit_index_root(&self, record: &mut [u8], entry_name: &str) -> io::Result<(u64, bool)> {
        let root_pos = find_index_root_attr(record)
            .ok_or_else(|| invalid_fs_data("directory has no $INDEX_ROOT"))?;
        let content_start = root_pos + resident_content_offset(record, root_pos)?;
        let header = content_start + 0x10;
        let entries_offset = read_u32_at(record, header)?;
        let used_size = read_u32_at(record, header + 4)?;
        let region_start = (header as usize)
            .checked_add(entries_offset as usize)
            .ok_or_else(|| invalid_fs_data("index entries offset overflow"))?;
        let region_end = (header as usize)
            .checked_add(used_size as usize)
            .ok_or_else(|| invalid_fs_data("index used size overflow"))?;
        if region_end > record.len() || region_start >= region_end {
            return Err(invalid_fs_data("index root range out of bounds"));
        }
        let (entry_start, entry_len, child_ref, is_dir) =
            find_entry_span(&record[region_start..region_end], entry_name)?
                .ok_or_else(|| invalid_fs_data("index entry disappeared during edit"))?;
        let absolute = region_start + entry_start;
        record.copy_within(absolute + entry_len..region_end, absolute);
        for byte in &mut record[region_end - entry_len..region_end] {
            *byte = 0;
        }
        let new_used = (used_size as usize)
            .checked_sub(entry_len)
            .ok_or_else(|| invalid_fs_data("index used size underflow"))?;
        record[header + 4..header + 8].copy_from_slice(&(new_used as u32).to_le_bytes());
        Ok((child_ref, is_dir))
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        let mut reader = self.reader.borrow_mut();
        reader.seek(std::io::SeekFrom::Start(offset))?;
        reader.read_exact(buffer)
    }

    fn bytes_per_sector(&self) -> usize {
        self.bytes_per_sector as usize
    }
}

enum EntrySite {
    IndexRoot,
    IndexBlock { disk_offset: u64 },
}

fn edit_index_block(block: &mut [u8], entry_name: &str) -> io::Result<(u64, bool)> {
    let region = index_block_entries_region(block)?;
    let entries_offset = read_u32_at(block, 0x18)?;
    let used_size = read_u32_at(block, 0x1C)?;
    let (entry_start, entry_len, child_ref, is_dir) = find_entry_span(&block[region], entry_name)?
        .ok_or_else(|| invalid_fs_data("index entry disappeared during edit"))?;
    let absolute = 0x18 + entries_offset as usize + entry_start;
    let region_end = 0x18 + used_size as usize;
    block.copy_within(absolute + entry_len..region_end, absolute);
    for byte in &mut block[region_end - entry_len..region_end] {
        *byte = 0;
    }
    let new_used = (used_size as usize)
        .checked_sub(entry_len)
        .ok_or_else(|| invalid_fs_data("index used size underflow"))?;
    block[0x1C..0x20].copy_from_slice(&(new_used as u32).to_le_bytes());
    Ok((child_ref, is_dir))
}

fn index_block_entries_region(block: &[u8]) -> io::Result<std::ops::Range<usize>> {
    let entries_offset = read_u32_at(block, 0x18)? as usize;
    let used_size = read_u32_at(block, 0x1C)? as usize;
    let start = 0x18usize
        .checked_add(entries_offset)
        .ok_or_else(|| invalid_fs_data("index entries offset overflow"))?;
    let end = 0x18usize
        .checked_add(used_size)
        .ok_or_else(|| invalid_fs_data("index used size overflow"))?;
    if end > block.len() || start >= end {
        return Err(invalid_fs_data("index block entries out of bounds"));
    }
    Ok(start..end)
}

fn index_entries_region(content: &[u8]) -> io::Result<std::ops::Range<usize>> {
    if content.len() < 0x20 {
        return Err(invalid_fs_data("index root content too short"));
    }
    let entries_offset = read_u32_at(content, 0x10)? as usize;
    let used_size = read_u32_at(content, 0x14)? as usize;
    let start = 0x10usize
        .checked_add(entries_offset)
        .ok_or_else(|| invalid_fs_data("index entries offset overflow"))?;
    let end = 0x10usize
        .checked_add(used_size)
        .ok_or_else(|| invalid_fs_data("index used size overflow"))?;
    if end > content.len() || start >= end {
        return Err(invalid_fs_data("index root entries out of bounds"));
    }
    Ok(start..end)
}

fn index_root_has_allocation(content: &[u8]) -> bool {
    content.len() >= 0x20 && content[0x1C] & 0x01 != 0
}

/// Walk an entry list and return (start, length, child MFT reference, is_dir)
/// of the entry carrying `entry_name`.
fn find_entry_span(
    entries: &[u8],
    entry_name: &str,
) -> io::Result<Option<(usize, usize, u64, bool)>> {
    let mut offset = 0usize;
    while offset + 0x52 <= entries.len() {
        let entry_len = u16::from_le_bytes([entries[offset + 8], entries[offset + 9]]) as usize;
        if entry_len < 0x52 || offset + entry_len > entries.len() {
            break;
        }
        let flags = u32::from_le_bytes(
            entries[offset + 0x0C..offset + 0x10]
                .try_into()
                .unwrap_or([0; 4]),
        );
        if flags & INDEX_ENTRY_TERMINATOR != 0 {
            break;
        }
        let name_len = entries[offset + 0x50] as usize;
        let name_start = offset + 0x52;
        if name_start + name_len * 2 > offset + entry_len {
            break;
        }
        let name_units: Vec<u16> = entries[name_start..name_start + name_len * 2]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        let name = String::from_utf16_lossy(&name_units);
        if name == entry_name || name.to_uppercase() == entry_name.to_uppercase() {
            if flags & INDEX_ENTRY_HAS_CHILD != 0 {
                return Err(invalid_fs_data(
                    "index entry has a child sub-index; refusing to remove it",
                ));
            }
            let file_ref =
                u64::from_le_bytes(entries[offset..offset + 8].try_into().unwrap_or([0; 8]));
            let file_flags = u32::from_le_bytes(
                entries[offset + 0x48..offset + 0x4C]
                    .try_into()
                    .unwrap_or([0; 4]),
            );
            return Ok(Some((
                offset,
                entry_len,
                file_ref & 0x0000_FFFF_FFFF_FFFF,
                file_flags & 0x1000_0000 != 0,
            )));
        }
        offset += entry_len;
    }
    Ok(None)
}

fn find_index_root_attr(record: &[u8]) -> Option<usize> {
    let mut pos = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    while pos + 8 < record.len() {
        let attr_type = u32::from_le_bytes(record[pos..pos + 4].try_into().ok()?);
        if attr_type == 0xFFFF_FFFF {
            break;
        }
        let length = attr_len_at(record, pos).ok()?;
        if length == 0 || pos + length > record.len() {
            break;
        }
        if attr_type == ATTR_TYPE_INDEX_ROOT {
            return Some(pos);
        }
        pos += length;
    }
    None
}

fn attr_len_at(record: &[u8], pos: usize) -> io::Result<usize> {
    Ok(u32::from_le_bytes(
        record
            .get(pos + 4..pos + 8)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| invalid_fs_data("attribute header out of bounds"))?,
    ) as usize)
}

fn resident_content_offset(record: &[u8], pos: usize) -> io::Result<usize> {
    let offset = u16::from_le_bytes(
        record
            .get(pos + 0x14..pos + 0x16)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| invalid_fs_data("attribute content offset out of bounds"))?,
    ) as usize;
    Ok(pos + offset)
}

fn read_u32_at(data: &[u8], offset: usize) -> io::Result<u32> {
    data.get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| invalid_fs_data("index field out of bounds"))
}

/// Rebuild the on-disk image of a FILE/INDX record from its logical
/// (fixup-applied) content: refresh the USA saved words from the current
/// sector tails, then stamp the sequence number back onto the tails.
fn inverse_record_fixup(record: &mut [u8], sector_size: usize) -> io::Result<()> {
    if record.len() < 8 || sector_size < 2 {
        return Ok(());
    }
    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_offset == 0 || usa_count < 2 {
        return Ok(());
    }
    if usa_offset + usa_count * 2 > record.len() {
        return Err(invalid_fs_data("update sequence array exceeds record"));
    }
    let sequence = [record[usa_offset], record[usa_offset + 1]];
    for index in 1..usa_count {
        let tail = index
            .checked_mul(sector_size)
            .and_then(|value| value.checked_sub(2))
            .ok_or_else(|| invalid_fs_data("invalid fixup position"))?;
        if tail + 2 > record.len() {
            return Err(invalid_fs_data("record too short for fixup"));
        }
        record[usa_offset + index * 2] = record[tail];
        record[usa_offset + index * 2 + 1] = record[tail + 1];
        record[tail] = sequence[0];
        record[tail + 1] = sequence[1];
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/namespace_edit.rs"]
mod tests;
