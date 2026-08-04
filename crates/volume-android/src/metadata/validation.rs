use crate::error::{Result, VolumeAndroidError};
use crate::geometry::LpGeometry;
use crate::metadata::{BlockDevice, LogicalExtentTarget, LogicalPartition};
use crate::LP_SECTOR_SIZE;

pub(super) fn validate_super_device(
    geometry: LpGeometry,
    source_size: u64,
    devices: &[BlockDevice],
) -> Result<()> {
    let super_device = &devices[0];
    let first_data_byte = super_device
        .first_logical_sector
        .checked_mul(LP_SECTOR_SIZE)
        .ok_or(VolumeAndroidError::ArithmeticOverflow("first logical byte"))?;
    if geometry.total_metadata_size()? > first_data_byte {
        return Err(VolumeAndroidError::InvalidMetadata(
            "liblp metadata overlaps logical partition data".to_string(),
        ));
    }
    if super_device.size > source_size {
        return Err(VolumeAndroidError::InvalidMetadata(
            "super block-device size exceeds the supplied image".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_physical_extents(partitions: &[LogicalPartition]) -> Result<()> {
    let mut ranges = Vec::new();
    for partition in partitions {
        for extent in &partition.extents {
            if let LogicalExtentTarget::Linear {
                source_index,
                source_offset,
            } = extent.target
            {
                let end = source_offset.checked_add(extent.length).ok_or(
                    VolumeAndroidError::ArithmeticOverflow("physical extent end"),
                )?;
                ranges.push((source_index, source_offset, end, partition.name.as_str()));
            }
        }
    }
    ranges.sort_unstable_by_key(|range| (range.0, range.1));
    for pair in ranges.windows(2) {
        if pair[0].0 == pair[1].0 && pair[0].2 > pair[1].1 {
            return Err(VolumeAndroidError::InvalidMetadata(format!(
                "physical extents for `{}` and `{}` overlap",
                pair[0].3, pair[1].3
            )));
        }
    }
    Ok(())
}
