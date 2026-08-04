use std::collections::HashSet;

use crate::bytes::{read_name, read_u32, read_u64};
use crate::error::{Result, VolumeAndroidError};
use crate::metadata::header::{MetadataHeader, BLOCK_DEVICE_ENTRY_SIZE, EXTENT_ENTRY_SIZE};
use crate::metadata::{BlockDevice, LogicalExtent, LogicalExtentTarget, LogicalPartition};
use crate::LP_SECTOR_SIZE;

const PARTITION_ENTRY_SIZE: usize = 52;
const PARTITION_ATTR_SLOT_SUFFIXED: u32 = 1 << 1;
const PARTITION_ATTR_DISABLED: u32 = 1 << 3;

#[derive(Debug, Clone, Copy)]
struct WirePartition {
    attributes: u32,
    first_extent_index: u32,
    num_extents: u32,
    group_index: u32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct WireExtent {
    num_sectors: u64,
    target_type: u32,
    target_data: u64,
    target_source: u32,
}

pub(super) fn parse_partitions(
    bytes: &[u8],
    header: &MetadataHeader,
    slot_number: u32,
    wire_extents: &[WireExtent],
    devices: &[BlockDevice],
) -> Result<Vec<LogicalPartition>> {
    let mut names = HashSet::new();
    let mut owned_extents = vec![false; wire_extents.len()];
    let mut partitions = Vec::new();
    for entry in bytes.chunks_exact(PARTITION_ENTRY_SIZE) {
        let raw_name = read_name(&entry[..36], "logical partition name")?;
        let wire = WirePartition {
            attributes: read_u32(entry, 36, "partition attributes")?,
            first_extent_index: read_u32(entry, 40, "first extent index")?,
            num_extents: read_u32(entry, 44, "extent count")?,
            group_index: read_u32(entry, 48, "group index")?,
        };
        validate_partition_wire(header, wire)?;
        if wire.group_index >= header.groups.num_entries {
            return Err(VolumeAndroidError::InvalidMetadata(format!(
                "partition `{raw_name}` references missing group {}",
                wire.group_index
            )));
        }
        let name = apply_slot_suffix(&raw_name, wire.attributes, slot_number);
        if !names.insert(name.clone()) {
            return Err(VolumeAndroidError::InvalidMetadata(format!(
                "duplicate logical partition name `{name}`"
            )));
        }
        let extents = partition_extents(&name, wire, wire_extents, devices, &mut owned_extents)?;
        let size = extents.iter().try_fold(0u64, |size, extent| {
            size.checked_add(extent.length)
                .ok_or(VolumeAndroidError::ArithmeticOverflow("partition size"))
        })?;
        partitions.push(LogicalPartition {
            name,
            attributes: wire.attributes,
            group_index: wire.group_index,
            size,
            disabled: wire.attributes & PARTITION_ATTR_DISABLED != 0,
            extents,
        });
    }
    if owned_extents.iter().any(|owned| !owned) {
        return Err(VolumeAndroidError::InvalidMetadata(
            "extent table contains an unowned entry".to_string(),
        ));
    }
    Ok(partitions)
}

fn partition_extents(
    partition: &str,
    wire: WirePartition,
    all_extents: &[WireExtent],
    devices: &[BlockDevice],
    owned: &mut [bool],
) -> Result<Vec<LogicalExtent>> {
    let start = wire.first_extent_index as usize;
    let end = start.checked_add(wire.num_extents as usize).ok_or(
        VolumeAndroidError::ArithmeticOverflow("partition extent range"),
    )?;
    let selected = all_extents.get(start..end).ok_or_else(|| {
        VolumeAndroidError::InvalidMetadata(format!(
            "partition `{partition}` extent range is outside the table"
        ))
    })?;
    let mut logical_offset = 0u64;
    let mut result = Vec::with_capacity(selected.len());
    for (relative_index, wire_extent) in selected.iter().enumerate() {
        let index = start + relative_index;
        if owned[index] {
            return Err(VolumeAndroidError::InvalidMetadata(format!(
                "extent {index} is owned by more than one partition"
            )));
        }
        owned[index] = true;
        let extent = map_extent(partition, logical_offset, *wire_extent, devices)?;
        logical_offset = logical_offset.checked_add(extent.length).ok_or(
            VolumeAndroidError::ArithmeticOverflow("logical extent offset"),
        )?;
        result.push(extent);
    }
    Ok(result)
}

fn map_extent(
    partition: &str,
    logical_offset: u64,
    wire: WireExtent,
    devices: &[BlockDevice],
) -> Result<LogicalExtent> {
    let length = wire
        .num_sectors
        .checked_mul(LP_SECTOR_SIZE)
        .ok_or(VolumeAndroidError::ArithmeticOverflow("extent length"))?;
    if length == 0 {
        return Err(VolumeAndroidError::InvalidMetadata(format!(
            "partition `{partition}` has a zero-length extent"
        )));
    }
    let target = match wire.target_type {
        0 => linear_target(partition, wire, length, devices)?,
        1 if wire.target_data == 0 && wire.target_source == 0 => LogicalExtentTarget::Zero,
        1 => {
            return Err(VolumeAndroidError::InvalidMetadata(format!(
                "partition `{partition}` has a malformed zero extent"
            )));
        }
        other => {
            return Err(VolumeAndroidError::InvalidMetadata(format!(
                "partition `{partition}` uses unknown target type {other}"
            )));
        }
    };
    Ok(LogicalExtent {
        logical_offset,
        length,
        target,
    })
}

fn linear_target(
    partition: &str,
    wire: WireExtent,
    length: u64,
    devices: &[BlockDevice],
) -> Result<LogicalExtentTarget> {
    let device = devices.get(wire.target_source as usize).ok_or_else(|| {
        VolumeAndroidError::InvalidMetadata(format!(
            "partition `{partition}` references missing block device {}",
            wire.target_source
        ))
    })?;
    if wire.target_data < device.first_logical_sector {
        return Err(VolumeAndroidError::InvalidMetadata(format!(
            "partition `{partition}` extent overlaps liblp metadata"
        )));
    }
    let source_offset = wire.target_data.checked_mul(LP_SECTOR_SIZE).ok_or(
        VolumeAndroidError::ArithmeticOverflow("linear extent offset"),
    )?;
    let end = source_offset
        .checked_add(length)
        .ok_or(VolumeAndroidError::ArithmeticOverflow("linear extent end"))?;
    if end > device.size {
        return Err(VolumeAndroidError::InvalidMetadata(format!(
            "partition `{partition}` extent exceeds block device `{}`",
            device.partition_name
        )));
    }
    Ok(LogicalExtentTarget::Linear {
        source_index: wire.target_source,
        source_offset,
    })
}

pub(super) fn parse_extents(bytes: &[u8]) -> Result<Vec<WireExtent>> {
    bytes
        .chunks_exact(EXTENT_ENTRY_SIZE as usize)
        .map(|entry| {
            Ok(WireExtent {
                num_sectors: read_u64(entry, 0, "extent sector count")?,
                target_type: read_u32(entry, 8, "extent target type")?,
                target_data: read_u64(entry, 12, "extent target data")?,
                target_source: read_u32(entry, 20, "extent target source")?,
            })
        })
        .collect()
}

pub(super) fn parse_block_devices(bytes: &[u8]) -> Result<Vec<BlockDevice>> {
    let devices: Result<Vec<_>> = bytes
        .chunks_exact(BLOCK_DEVICE_ENTRY_SIZE as usize)
        .map(|entry| {
            Ok(BlockDevice {
                first_logical_sector: read_u64(entry, 0, "first logical sector")?,
                alignment: read_u32(entry, 8, "block-device alignment")?,
                alignment_offset: read_u32(entry, 12, "block-device alignment offset")?,
                size: read_u64(entry, 16, "block-device size")?,
                partition_name: read_name(&entry[24..60], "block-device name")?,
                flags: read_u32(entry, 60, "block-device flags")?,
            })
        })
        .collect();
    let devices = devices?;
    if devices.is_empty() {
        return Err(VolumeAndroidError::InvalidMetadata(
            "block-device table is empty".to_string(),
        ));
    }
    Ok(devices)
}

fn validate_partition_wire(header: &MetadataHeader, wire: WirePartition) -> Result<()> {
    if wire.num_extents == 0 {
        return Err(VolumeAndroidError::InvalidMetadata(
            "logical partition has no extents".to_string(),
        ));
    }
    let allowed = if header.minor_version >= 1 {
        0x0f
    } else {
        0x03
    };
    if wire.attributes & !allowed != 0 {
        return Err(VolumeAndroidError::InvalidMetadata(format!(
            "partition attributes 0x{:x} are invalid for metadata version 10.{}",
            wire.attributes, header.minor_version
        )));
    }
    Ok(())
}

fn apply_slot_suffix(name: &str, attributes: u32, slot_number: u32) -> String {
    if attributes & PARTITION_ATTR_SLOT_SUFFIXED == 0 {
        return name.to_string();
    }
    let suffix = u32::from(b'a')
        .checked_add(slot_number)
        .and_then(char::from_u32)
        .map(|value| value.to_string())
        .unwrap_or_else(|| format!("slot{slot_number}"));
    format!("{name}_{suffix}")
}
