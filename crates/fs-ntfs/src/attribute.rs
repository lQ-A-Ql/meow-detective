//! NTFS attribute parsing and data extent helpers.

use crate::data_runs::{data_runs_logical_size, parse_data_runs_ext};
use crate::{invalid_fs_data, ATTR_TYPE_ATTRIBUTE_LIST, ATTR_TYPE_END, MAX_ATTRIBUTE_LIST_ENTRIES};
use std::io;

#[derive(Debug, Clone)]
pub enum DataAttributeExtent {
    Resident {
        data: Vec<u8>,
    },
    NonResident {
        lowest_vcn: u64,
        highest_vcn: u64,
        allocated_size: u64,
        real_size: u64,
        attr_flags: u16,
        compression_unit_exp: u16,
        runs: Vec<crate::DataRun>,
    },
}

#[derive(Debug)]
pub struct AttributeListEntry {
    pub attr_type: u32,
    pub name_len: u8,
    pub lowest_vcn: u64,
    pub name: Option<String>,
    pub record_number: u64,
    pub record_sequence: u16,
    pub attribute_id: u16,
}

pub(crate) fn resident_attr_content(
    record: &[u8],
    attr_pos: usize,
    attr_len: usize,
) -> Option<&[u8]> {
    if attr_pos + 0x16 > record.len() {
        return None;
    }
    if (record[attr_pos + 8] & 1) != 0 {
        return None;
    }

    let content_size =
        u32::from_le_bytes(record[attr_pos + 0x10..attr_pos + 0x14].try_into().ok()?) as usize;
    let content_off =
        u16::from_le_bytes(record[attr_pos + 0x14..attr_pos + 0x16].try_into().ok()?) as usize;
    let attr_end = attr_pos.checked_add(attr_len)?;
    let content_start = attr_pos.checked_add(content_off)?;
    let content_end = content_start.checked_add(content_size)?;

    if content_off < 0x18 || content_start >= attr_end || content_end > attr_end {
        return None;
    }

    record.get(content_start..content_end)
}

pub(crate) fn pos_is_nonresident(record: &[u8], attr_pos: usize) -> bool {
    attr_pos + 9 <= record.len() && (record[attr_pos + 8] & 1) != 0
}

pub(crate) fn nonresident_compression_unit(record: &[u8], attr_pos: usize) -> u16 {
    if attr_pos + 0x24 <= record.len() {
        u16::from_le_bytes(
            record[attr_pos + 0x22..attr_pos + 0x24]
                .try_into()
                .unwrap_or([0; 2]),
        )
    } else {
        4
    }
}

pub(crate) fn parse_data_attribute_extent(
    record: &[u8],
    attr_pos: usize,
    attr_len: usize,
) -> io::Result<Option<DataAttributeExtent>> {
    if pos_is_nonresident(record, attr_pos) {
        if attr_pos + 0x40 > record.len() {
            return Ok(None);
        }
        let run_off =
            u16::from_le_bytes([record[attr_pos + 0x20], record[attr_pos + 0x21]]) as usize;
        let attr_end = attr_pos
            .checked_add(attr_len)
            .ok_or_else(|| invalid_fs_data("attribute length overflow"))?
            .min(record.len());
        if run_off == 0 || attr_pos + run_off >= attr_end {
            return Ok(None);
        }

        let lowest_vcn = u64::from_le_bytes(
            record[attr_pos + 0x10..attr_pos + 0x18]
                .try_into()
                .unwrap_or([0; 8]),
        );
        let highest_vcn = u64::from_le_bytes(
            record[attr_pos + 0x18..attr_pos + 0x20]
                .try_into()
                .unwrap_or([0; 8]),
        );
        let allocated_size = u64::from_le_bytes(
            record[attr_pos + 0x28..attr_pos + 0x30]
                .try_into()
                .unwrap_or([0; 8]),
        );
        let real_size = u64::from_le_bytes(
            record[attr_pos + 0x30..attr_pos + 0x38]
                .try_into()
                .unwrap_or([0; 8]),
        );
        let attr_flags = u16::from_le_bytes(
            record[attr_pos + 0x0c..attr_pos + 0x0e]
                .try_into()
                .unwrap_or([0; 2]),
        );
        let compression_unit_exp = nonresident_compression_unit(record, attr_pos);
        let runs = parse_data_runs_ext(&record[attr_pos + run_off..attr_end])?;
        return Ok(Some(DataAttributeExtent::NonResident {
            lowest_vcn,
            highest_vcn,
            allocated_size,
            real_size,
            attr_flags,
            compression_unit_exp,
            runs,
        }));
    }

    let Some(content) = resident_attr_content(record, attr_pos, attr_len) else {
        return Ok(None);
    };
    Ok(Some(DataAttributeExtent::Resident {
        data: content.to_vec(),
    }))
}

pub(crate) fn parse_attribute_list_entries(mut data: &[u8]) -> io::Result<Vec<AttributeListEntry>> {
    let mut entries = Vec::new();
    while data.len() >= 0x1a {
        if entries.len() >= MAX_ATTRIBUTE_LIST_ENTRIES {
            return Err(invalid_fs_data(
                "NTFS attribute list exceeds the entry safety limit",
            ));
        }
        let attr_type = u32::from_le_bytes(data[0..4].try_into().unwrap_or([0; 4]));
        if attr_type == ATTR_TYPE_END {
            break;
        }

        let entry_len = u16::from_le_bytes(data[4..6].try_into().unwrap_or([0; 2])) as usize;
        if entry_len < 0x1a || entry_len > data.len() {
            return Err(invalid_fs_data("invalid NTFS attribute-list entry length"));
        }

        let name_len = data[6];
        let name_off = data[7] as usize;
        let name_bytes = (name_len as usize)
            .checked_mul(2)
            .ok_or_else(|| invalid_fs_data("NTFS attribute-list name length overflow"))?;
        if name_len > 0
            && (name_off < 0x1a
                || name_off
                    .checked_add(name_bytes)
                    .is_none_or(|end| end > entry_len))
        {
            return Err(invalid_fs_data("invalid NTFS attribute-list name range"));
        }

        let name = if name_len == 0 {
            None
        } else {
            let name_end = name_off + name_bytes;
            let chars = data[name_off..name_end]
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>();
            Some(String::from_utf16_lossy(&chars))
        };

        let lowest_vcn = u64::from_le_bytes(data[8..0x10].try_into().unwrap_or([0; 8]));
        let file_reference = u64::from_le_bytes(data[0x10..0x18].try_into().unwrap_or([0; 8]));
        entries.push(AttributeListEntry {
            attr_type,
            name_len,
            lowest_vcn,
            name,
            record_number: file_reference & 0x0000_FFFF_FFFF_FFFF,
            record_sequence: (file_reference >> 48) as u16,
            attribute_id: u16::from_le_bytes(data[0x18..0x1a].try_into().unwrap_or([0; 2])),
        });
        data = &data[entry_len..];
    }

    if !data.is_empty() && !data.iter().all(|byte| *byte == 0) {
        return Err(invalid_fs_data(
            "truncated trailing bytes in NTFS attribute list",
        ));
    }
    Ok(entries)
}

pub(crate) fn sort_data_extents(extents: &mut [DataAttributeExtent]) {
    extents.sort_by_key(|extent| match extent {
        DataAttributeExtent::Resident { .. } => 0,
        DataAttributeExtent::NonResident { lowest_vcn, .. } => *lowest_vcn,
    });
}

pub(crate) fn data_extents_logical_size(
    extents: &[DataAttributeExtent],
    cluster_size: u64,
) -> io::Result<u64> {
    let mut size = 0u64;
    for extent in extents {
        let start = data_extent_logical_start(extent, cluster_size)?;
        let len = data_extent_logical_len(extent, cluster_size)?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| invalid_fs_data("data extent logical size overflow"))?;
        size = size.max(end);
    }
    Ok(size)
}

pub(crate) fn data_extents_declared_size(
    extents: &[DataAttributeExtent],
    cluster_size: u64,
) -> io::Result<u64> {
    let mut declared = 0u64;
    for extent in extents {
        match extent {
            DataAttributeExtent::Resident { data } => {
                declared = declared.max(
                    u64::try_from(data.len())
                        .map_err(|_| invalid_fs_data("resident data length overflow"))?,
                );
            }
            DataAttributeExtent::NonResident {
                lowest_vcn,
                real_size,
                ..
            } => {
                if *lowest_vcn == 0 {
                    declared = declared.max(*real_size);
                }
            }
        }
    }

    if declared == 0 {
        data_extents_logical_size(extents, cluster_size)
    } else {
        Ok(declared)
    }
}

pub(crate) fn data_extent_logical_start(
    extent: &DataAttributeExtent,
    cluster_size: u64,
) -> io::Result<u64> {
    match extent {
        DataAttributeExtent::Resident { .. } => Ok(0),
        DataAttributeExtent::NonResident { lowest_vcn, .. } => lowest_vcn
            .checked_mul(cluster_size)
            .ok_or_else(|| invalid_fs_data("data extent logical offset overflow")),
    }
}

pub(crate) fn data_extent_logical_len(
    extent: &DataAttributeExtent,
    cluster_size: u64,
) -> io::Result<u64> {
    match extent {
        DataAttributeExtent::Resident { data } => {
            u64::try_from(data.len()).map_err(|_| invalid_fs_data("resident data length overflow"))
        }
        DataAttributeExtent::NonResident {
            allocated_size,
            real_size,
            lowest_vcn,
            runs,
            ..
        } => {
            let allocated = data_runs_logical_size(runs, cluster_size)?;
            let allocated = if *allocated_size > 0 {
                (*allocated_size).min(allocated)
            } else {
                allocated
            };
            if *lowest_vcn == 0 {
                Ok((*real_size).min(allocated))
            } else {
                Ok(allocated)
            }
        }
    }
}

pub(crate) fn is_unnamed_attribute(record: &[u8], attr_pos: usize) -> bool {
    attr_pos + 0x0a <= record.len() && record[attr_pos + 0x09] == 0
}

pub(crate) fn attribute_name_matches(
    record: &[u8],
    attr_pos: usize,
    attr_len: usize,
    expected: Option<&str>,
) -> bool {
    let Some(attr_end) = attr_pos.checked_add(attr_len) else {
        return false;
    };
    if attr_pos + 0x0c > record.len() || attr_end > record.len() {
        return false;
    }
    let name_len = record[attr_pos + 0x09] as usize;
    if name_len == 0 {
        return expected.is_none();
    }
    let Some(expected) = expected else {
        return false;
    };
    let name_off = u16::from_le_bytes([record[attr_pos + 0x0a], record[attr_pos + 0x0b]]) as usize;
    let Some(name_start) = attr_pos.checked_add(name_off) else {
        return false;
    };
    let Some(name_end) = name_start.checked_add(name_len.saturating_mul(2)) else {
        return false;
    };
    if name_start < attr_pos || name_end > attr_end {
        return false;
    }
    let chars = record[name_start..name_end]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&chars).eq_ignore_ascii_case(expected)
}

impl crate::NtfsReader {
    pub(crate) fn attribute_list_entries(
        &self,
        record: &[u8],
    ) -> io::Result<Option<Vec<AttributeListEntry>>> {
        if record.len() < 0x18 {
            return Err(invalid_fs_data(
                "FILE record is too short for an attribute list",
            ));
        }
        let mut entries = Vec::new();
        let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
        let mut pos = attr_off;
        let mut saw_attribute_list = false;

        while pos + 8 < record.len() {
            let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == ATTR_TYPE_END {
                break;
            }
            let len =
                u32::from_le_bytes(record[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos.checked_add(len).is_none_or(|end| end > record.len()) {
                if typ == ATTR_TYPE_ATTRIBUTE_LIST {
                    return Err(invalid_fs_data("invalid NTFS attribute-list attribute"));
                }
                break;
            }

            if typ == ATTR_TYPE_ATTRIBUTE_LIST {
                saw_attribute_list = true;
                let attr_entries = self.read_attribute_list_content(record, pos, len)?;
                for entry in attr_entries {
                    if entries.len() >= MAX_ATTRIBUTE_LIST_ENTRIES {
                        return Err(invalid_fs_data(
                            "NTFS attribute list exceeds the entry safety limit",
                        ));
                    }
                    entries.push(entry);
                }
            }

            pos += len;
        }

        Ok(saw_attribute_list.then_some(entries))
    }

    fn read_attribute_list_content(
        &self,
        record: &[u8],
        attr_pos: usize,
        attr_len: usize,
    ) -> io::Result<Vec<AttributeListEntry>> {
        let is_nonresident = pos_is_nonresident(record, attr_pos);
        if is_nonresident {
            if attr_len < 0x40
                || attr_pos
                    .checked_add(0x40)
                    .is_none_or(|end| end > record.len())
            {
                return Err(invalid_fs_data(
                    "non-resident NTFS attribute-list header is truncated",
                ));
            }
            let content = self.read_attr_nonresident(attr_pos, record)?;
            return parse_attribute_list_entries(&content);
        }

        let content = resident_attr_content(record, attr_pos, attr_len)
            .ok_or_else(|| invalid_fs_data("invalid resident NTFS attribute-list content range"))?;
        parse_attribute_list_entries(content)
    }
}

pub(crate) fn optional_name_matches(actual: Option<&str>, expected: Option<&str>) -> bool {
    match (actual, expected) {
        (None, None) => true,
        (Some(actual), Some(expected)) => actual.eq_ignore_ascii_case(expected),
        _ => false,
    }
}

#[cfg(test)]
#[path = "../tests/unit/attribute.rs"]
mod tests;
