use std::collections::{HashMap, HashSet};
use std::io;

use crate::attribute::{
    attribute_name_matches, optional_name_matches, parse_data_attribute_extent, sort_data_extents,
    AttributeListEntry, DataAttributeExtent,
};
use crate::utils::{file_record_sequence, is_extension_record_for, validate_file_record};
use crate::{invalid_fs_data, ATTR_TYPE_DATA, ATTR_TYPE_END, MAX_EXTERNAL_ATTRIBUTE_RECORDS};

impl crate::NtfsReader {
    pub(crate) fn collect_unnamed_data_extents(
        &self,
        inode: u64,
    ) -> io::Result<Vec<DataAttributeExtent>> {
        let record = self.read_mft_record(inode)?;
        self.collect_unnamed_data_extents_from_base(inode, &record)
    }

    pub(super) fn collect_unnamed_data_extents_from_base(
        &self,
        inode: u64,
        record: &[u8],
    ) -> io::Result<Vec<DataAttributeExtent>> {
        self.collect_attribute_extents_from_base(inode, record, ATTR_TYPE_DATA, None)
    }

    pub(crate) fn collect_attribute_extents_from_base(
        &self,
        inode: u64,
        record: &[u8],
        attribute_type: u32,
        attribute_name: Option<&str>,
    ) -> io::Result<Vec<DataAttributeExtent>> {
        validate_file_record(record, inode)?;
        let mut extents = match self.attribute_list_entries(record)? {
            Some(entries) => {
                self.collect_listed_extents(inode, record, entries, attribute_type, attribute_name)?
            }
            None => collect_matching_extents(record, attribute_type, attribute_name)?,
        };
        sort_data_extents(&mut extents);
        validate_extent_chain(&extents)?;
        Ok(extents)
    }

    fn collect_listed_extents(
        &self,
        inode: u64,
        base_record: &[u8],
        entries: Vec<AttributeListEntry>,
        attribute_type: u32,
        attribute_name: Option<&str>,
    ) -> io::Result<Vec<DataAttributeExtent>> {
        let base_sequence = file_record_sequence(base_record)
            .ok_or_else(|| invalid_fs_data("base FILE record has no sequence number"))?;
        let entries = entries
            .into_iter()
            .filter(|entry| {
                entry.attr_type == attribute_type
                    && optional_name_matches(entry.name.as_deref(), attribute_name)
            })
            .collect::<Vec<_>>();
        validate_external_record_limit(inode, &entries)?;

        let mut records = HashMap::<u64, Vec<u8>>::new();
        let mut extents = Vec::with_capacity(entries.len());
        for entry in entries {
            let record = if entry.record_number == inode {
                base_record
            } else {
                if let std::collections::hash_map::Entry::Vacant(slot) =
                    records.entry(entry.record_number)
                {
                    slot.insert(self.read_mft_record(entry.record_number)?);
                }
                records
                    .get(&entry.record_number)
                    .map(Vec::as_slice)
                    .ok_or_else(|| invalid_fs_data("external FILE record cache lookup failed"))?
            };
            validate_list_record(record, inode, base_sequence, &entry)?;
            extents.push(collect_listed_extent(record, &entry, attribute_name)?);
        }
        Ok(extents)
    }
}

fn collect_matching_extents(
    record: &[u8],
    expected_type: u32,
    expected_name: Option<&str>,
) -> io::Result<Vec<DataAttributeExtent>> {
    let mut extents = Vec::new();
    for_attribute(record, |position, length, attribute_type| {
        if attribute_type == expected_type
            && attribute_name_matches(record, position, length, expected_name)
        {
            let extent = parse_data_attribute_extent(record, position, length)?
                .ok_or_else(|| invalid_fs_data("invalid NTFS data-bearing attribute"))?;
            extents.push(extent);
        }
        Ok(())
    })?;
    Ok(extents)
}

fn collect_listed_extent(
    record: &[u8],
    entry: &AttributeListEntry,
    expected_name: Option<&str>,
) -> io::Result<DataAttributeExtent> {
    let mut matched = None;
    for_attribute(record, |position, length, attribute_type| {
        if attribute_type != entry.attr_type
            || !attribute_name_matches(record, position, length, expected_name)
            || attribute_instance(record, position) != Some(entry.attribute_id)
            || attribute_lowest_vcn(record, position) != Some(entry.lowest_vcn)
        {
            return Ok(());
        }
        if matched.is_some() {
            return Err(invalid_fs_data(
                "duplicate NTFS attribute matches one attribute-list identity",
            ));
        }
        matched = parse_data_attribute_extent(record, position, length)?;
        Ok(())
    })?;
    matched.ok_or_else(|| {
        invalid_fs_data(format!(
            "attribute-list identity was not found in FILE record {}",
            entry.record_number
        ))
    })
}

fn for_attribute(
    record: &[u8],
    mut visitor: impl FnMut(usize, usize, u32) -> io::Result<()>,
) -> io::Result<()> {
    if record.len() < 0x18 {
        return Err(invalid_fs_data("FILE record is too short for attributes"));
    }
    let mut position = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    while position + 8 <= record.len() {
        let attribute_type =
            u32::from_le_bytes(record[position..position + 4].try_into().unwrap_or([0; 4]));
        if attribute_type == ATTR_TYPE_END {
            return Ok(());
        }
        if attribute_type == 0 && record[position..].iter().all(|byte| *byte == 0) {
            return Ok(());
        }
        let length = u32::from_le_bytes(
            record[position + 4..position + 8]
                .try_into()
                .unwrap_or([0; 4]),
        ) as usize;
        if length < 0x18
            || position
                .checked_add(length)
                .is_none_or(|end| end > record.len())
        {
            return Err(invalid_fs_data("invalid attribute length in FILE record"));
        }
        visitor(position, length, attribute_type)?;
        position += length;
    }
    Err(invalid_fs_data("FILE attribute list has no end marker"))
}

fn validate_list_record(
    record: &[u8],
    base_inode: u64,
    base_sequence: u16,
    entry: &AttributeListEntry,
) -> io::Result<()> {
    validate_file_record(record, entry.record_number)?;
    let actual_sequence = file_record_sequence(record)
        .ok_or_else(|| invalid_fs_data("attribute-list target has no sequence number"))?;
    if actual_sequence != entry.record_sequence {
        return Err(invalid_fs_data(format!(
            "attribute-list FILE sequence mismatch for record {}: expected {}, found {}",
            entry.record_number, entry.record_sequence, actual_sequence
        )));
    }
    if entry.record_number != base_inode
        && !is_extension_record_for(record, base_inode, base_sequence)
    {
        return Err(invalid_fs_data(format!(
            "attribute-list extension record {} has a mismatched base reference",
            entry.record_number
        )));
    }
    Ok(())
}

fn validate_external_record_limit(inode: u64, entries: &[AttributeListEntry]) -> io::Result<()> {
    let external = entries
        .iter()
        .filter(|entry| entry.record_number != inode)
        .map(|entry| entry.record_number)
        .collect::<HashSet<_>>();
    if external.len() > MAX_EXTERNAL_ATTRIBUTE_RECORDS {
        return Err(invalid_fs_data(format!(
            "NTFS external attribute record count exceeds {MAX_EXTERNAL_ATTRIBUTE_RECORDS}"
        )));
    }
    Ok(())
}

fn attribute_instance(record: &[u8], position: usize) -> Option<u16> {
    let bytes = record.get(position + 0x0e..position + 0x10)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

fn attribute_lowest_vcn(record: &[u8], position: usize) -> Option<u64> {
    if *record.get(position + 8)? == 0 {
        return Some(0);
    }
    let bytes = record.get(position + 0x10..position + 0x18)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

fn validate_extent_chain(extents: &[DataAttributeExtent]) -> io::Result<()> {
    let resident_count = extents
        .iter()
        .filter(|extent| matches!(extent, DataAttributeExtent::Resident { .. }))
        .count();
    if resident_count > 0 {
        return if extents.len() == 1 {
            Ok(())
        } else {
            Err(invalid_fs_data(
                "resident NTFS attribute cannot have multiple extents",
            ))
        };
    }

    let mut expected_vcn = 0u64;
    for extent in extents {
        let DataAttributeExtent::NonResident {
            lowest_vcn,
            highest_vcn,
            runs,
            ..
        } = extent
        else {
            continue;
        };
        if *lowest_vcn != expected_vcn {
            return Err(invalid_fs_data(format!(
                "NTFS extent VCN gap or overlap: expected {expected_vcn}, found {lowest_vcn}"
            )));
        }
        let clusters = runs.iter().try_fold(0u64, |total, run| {
            total
                .checked_add(run.cluster_count)
                .ok_or_else(|| invalid_fs_data("NTFS extent cluster count overflow"))
        })?;
        if clusters == 0 {
            return Err(invalid_fs_data("non-resident NTFS extent has no data runs"));
        }
        let computed_highest = lowest_vcn
            .checked_add(clusters - 1)
            .ok_or_else(|| invalid_fs_data("NTFS extent VCN range overflow"))?;
        if computed_highest != *highest_vcn {
            return Err(invalid_fs_data(format!(
                "NTFS extent highest VCN mismatch: declared {highest_vcn}, computed {computed_highest}"
            )));
        }
        expected_vcn = highest_vcn
            .checked_add(1)
            .ok_or_else(|| invalid_fs_data("NTFS extent VCN range overflow"))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/extents.rs"]
mod tests;
