//! Bounded NTFS technical inspection for forensic structure views.

use crate::{invalid_fs_data, NtfsReader};
use std::io;

const MAX_ATTRIBUTE_RAW_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicDataRun {
    pub header: u8,
    pub length_field_size: u8,
    pub offset_field_size: u8,
    pub cluster_count: u64,
    pub relative_lcn: Option<i64>,
    pub absolute_lcn: Option<i64>,
    pub logical_offset: u64,
    pub raw: Vec<u8>,
    pub physical_offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfsTechnicalAttribute {
    pub attribute_type: u32,
    pub name: Option<String>,
    pub instance: u16,
    pub non_resident: bool,
    pub allocated_size: Option<u64>,
    pub real_size: Option<u64>,
    pub initialized_size: Option<u64>,
    pub raw: Vec<u8>,
    pub data_runs: Vec<ForensicDataRun>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfsTechnicalFile {
    pub inode: u64,
    pub sequence_number: u16,
    pub flags: u16,
    pub parent_reference: Option<u64>,
    pub record_offset: u64,
    pub record_size: u32,
    pub record_raw: Vec<u8>,
    pub attributes: Vec<NtfsTechnicalAttribute>,
}

impl NtfsReader {
    /// Inspect one FILE record without changing the normal file read path.
    pub fn inspect_file_by_inode(&self, inode: u64) -> io::Result<NtfsTechnicalFile> {
        let record = self.read_mft_record(inode)?;
        if record.len() < 0x18 || &record[..4] != b"FILE" {
            return Err(invalid_fs_data("inode is not a valid NTFS FILE record"));
        }
        let record_offset = self.mft_record_source_offset(inode)?;
        let attributes = inspect_attributes(self, &record)?;
        let parent_reference = attributes
            .iter()
            .find_map(|attribute| {
                (attribute.attribute_type == 0x30).then(|| parse_file_name_parent(&attribute.raw))
            })
            .flatten();
        Ok(NtfsTechnicalFile {
            inode,
            sequence_number: u16::from_le_bytes([record[0x10], record[0x11]]),
            flags: u16::from_le_bytes([record[0x16], record[0x17]]),
            parent_reference,
            record_offset,
            record_size: self.mft_record_size,
            record_raw: record,
            attributes,
        })
    }
}

fn inspect_attributes(
    reader: &NtfsReader,
    record: &[u8],
) -> io::Result<Vec<NtfsTechnicalAttribute>> {
    let mut attributes = Vec::new();
    let mut position = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    while position + 8 <= record.len() {
        let attribute_type =
            u32::from_le_bytes(record[position..position + 4].try_into().unwrap_or([0; 4]));
        if attribute_type == crate::ATTR_TYPE_END {
            break;
        }
        let length = u32::from_le_bytes(
            record[position + 4..position + 8]
                .try_into()
                .unwrap_or([0; 4]),
        ) as usize;
        let end = position
            .checked_add(length)
            .ok_or_else(|| invalid_fs_data("NTFS attribute length overflow"))?;
        if length < 0x18 || end > record.len() {
            return Err(invalid_fs_data("invalid NTFS attribute range"));
        }
        let raw = record[position..end].to_vec();
        let non_resident = record[position + 8] & 1 != 0;
        let name = attribute_name(record, position, end)?;
        let instance = u16::from_le_bytes([record[position + 0x0e], record[position + 0x0f]]);
        let (allocated_size, real_size, initialized_size, data_runs) = if non_resident {
            if length < 0x40 {
                return Err(invalid_fs_data(
                    "non-resident NTFS attribute header is truncated",
                ));
            }
            let allocated =
                u64::from_le_bytes(record[position + 0x28..position + 0x30].try_into().unwrap());
            let real =
                u64::from_le_bytes(record[position + 0x30..position + 0x38].try_into().unwrap());
            let initialized =
                u64::from_le_bytes(record[position + 0x38..position + 0x40].try_into().unwrap());
            let run_offset =
                u16::from_le_bytes([record[position + 0x20], record[position + 0x21]]) as usize;
            let run_start = position
                .checked_add(run_offset)
                .ok_or_else(|| invalid_fs_data("NTFS data run offset overflow"))?;
            if run_start >= end {
                return Err(invalid_fs_data(
                    "NTFS data run list is outside the attribute",
                ));
            }
            let runs = crate::data_runs::parse_data_runs_forensic(
                &record[run_start..end],
                reader.cluster_size,
                reader.volume_offset,
            )?;
            (Some(allocated), Some(real), Some(initialized), runs)
        } else {
            (None, None, None, Vec::new())
        };
        let bounded_raw = raw.len().min(MAX_ATTRIBUTE_RAW_BYTES);
        attributes.push(NtfsTechnicalAttribute {
            attribute_type,
            name,
            instance,
            non_resident,
            allocated_size,
            real_size,
            initialized_size,
            raw: raw[..bounded_raw].to_vec(),
            data_runs,
        });
        position = end;
    }
    Ok(attributes)
}

fn attribute_name(record: &[u8], position: usize, end: usize) -> io::Result<Option<String>> {
    let name_len = record[position + 9] as usize;
    if name_len == 0 {
        return Ok(None);
    }
    let name_offset =
        u16::from_le_bytes([record[position + 0x0a], record[position + 0x0b]]) as usize;
    let start = position
        .checked_add(name_offset)
        .ok_or_else(|| invalid_fs_data("NTFS attribute name overflow"))?;
    let bytes = name_len
        .checked_mul(2)
        .ok_or_else(|| invalid_fs_data("NTFS attribute name overflow"))?;
    let finish = start
        .checked_add(bytes)
        .ok_or_else(|| invalid_fs_data("NTFS attribute name overflow"))?;
    if start < position || finish > end {
        return Err(invalid_fs_data(
            "NTFS attribute name is outside the attribute",
        ));
    }
    let chars = record[start..finish]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    Ok(Some(String::from_utf16_lossy(&chars)))
}

fn parse_file_name_parent(attribute: &[u8]) -> Option<u64> {
    if attribute.len() < 0x16 || attribute[8] & 1 != 0 {
        return None;
    }
    let content_size = u32::from_le_bytes(attribute[0x10..0x14].try_into().ok()?) as usize;
    let content_offset = u16::from_le_bytes(attribute[0x14..0x16].try_into().ok()?) as usize;
    let content_end = content_offset.checked_add(content_size)?;
    let content = attribute.get(content_offset..content_end)?;
    Some(u64::from_le_bytes(content.get(..8)?.try_into().ok()?) & 0x0000_FFFF_FFFF_FFFF)
}

#[cfg(test)]
#[path = "../tests/unit/technical.rs"]
mod tests;
