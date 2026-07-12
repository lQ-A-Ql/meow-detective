use super::DiscoveredPv;
use crate::error::{LvmError, Result};
use crate::metadata::{LvMeta, PvMeta};
use crate::LvExtent;

pub(super) struct ResolvedPvMapping {
    pub(super) name: String,
    pub(super) start_offset: u64,
    pub(super) data_offset: u64,
    pub(super) data_size: u64,
    pub(super) pv_size: u64,
}

pub(super) fn resolve_pv_mapping(
    pv_meta: &PvMeta,
    matched: &DiscoveredPv,
) -> Result<ResolvedPvMapping> {
    let pe_start_bytes = pv_meta
        .pe_start
        .checked_mul(512)
        .ok_or_else(|| metadata_error(format!("PV '{}' pe_start overflows bytes", pv_meta.name)))?;
    let data_area = matched.label.data_areas.first().ok_or_else(|| {
        metadata_error(format!(
            "PV '{}' ({}) has no data area descriptor",
            pv_meta.name, pv_meta.uuid
        ))
    })?;
    if data_area.offset != pe_start_bytes {
        return Err(metadata_error(format!(
            "PV '{}' ({}) data area mismatch: label offset {} but metadata pe_start {} sectors = {} bytes",
            pv_meta.name, pv_meta.uuid, data_area.offset, pv_meta.pe_start, pe_start_bytes
        )));
    }
    let data_offset = matched
        .pv_offset
        .checked_add(data_area.offset)
        .ok_or_else(|| {
            metadata_error(format!("PV '{}' data offset overflows bytes", pv_meta.name))
        })?;
    let data_size = if data_area.size == 0 {
        matched
            .label
            .pv_size
            .checked_sub(data_area.offset)
            .ok_or_else(|| {
                metadata_error(format!(
                    "PV '{}' data area offset {} exceeds PV size {}",
                    pv_meta.name, data_area.offset, matched.label.pv_size
                ))
            })?
    } else {
        data_area.size
    };
    if data_area.offset.saturating_add(data_size) > matched.label.pv_size {
        return Err(metadata_error(format!(
            "PV '{}' data area range offset={} size={} exceeds PV size {}",
            pv_meta.name, data_area.offset, data_size, matched.label.pv_size
        )));
    }
    Ok(ResolvedPvMapping {
        name: pv_meta.name.clone(),
        start_offset: matched.pv_offset,
        data_offset,
        data_size,
        pv_size: matched.label.pv_size,
    })
}

pub(super) fn validate_extent_map(
    logical_volume: &LvMeta,
    extent_map: &[LvExtent],
    pv_bounds: &[(String, u64, u64, u64)],
) -> Result<()> {
    if logical_volume.size_bytes == 0 {
        return Ok(());
    }
    if extent_map.is_empty() {
        return Err(metadata_error(format!(
            "logical volume '{}' has no extent mappings",
            logical_volume.name
        )));
    }

    let mut expected = 0u64;
    for extent in extent_map {
        if extent.logical_start != expected {
            return Err(metadata_error(format!(
                "logical volume '{}' extent map has gap/overlap: expected logical offset {} but found {}",
                logical_volume.name, expected, extent.logical_start
            )));
        }
        expected = expected.checked_add(extent.length).ok_or_else(|| {
            metadata_error(format!(
                "logical volume '{}' extent map overflows",
                logical_volume.name
            ))
        })?;
        let Some((pv_name, data_start, data_size, pv_size)) = pv_bounds.get(extent.pv_index) else {
            return Err(metadata_error(format!(
                "logical volume '{}' extent references missing PV index {}",
                logical_volume.name, extent.pv_index
            )));
        };
        let data_end = data_start
            .checked_add(*data_size)
            .ok_or_else(|| metadata_error(format!("PV '{pv_name}' data area end overflows")))?;
        let extent_end = extent
            .physical_offset
            .checked_add(extent.length)
            .ok_or_else(|| {
                metadata_error(format!(
                    "logical volume '{}' physical extent overflows",
                    logical_volume.name
                ))
            })?;
        if extent.physical_offset < *data_start || extent_end > data_end {
            return Err(metadata_error(format!(
                "logical volume '{}' extent {}..{} falls outside PV '{}' data area {}..{} (pv size {})",
                logical_volume.name,
                extent.physical_offset,
                extent_end,
                pv_name,
                data_start,
                data_end,
                pv_size
            )));
        }
    }
    if expected != logical_volume.size_bytes {
        return Err(metadata_error(format!(
            "logical volume '{}' extent map covers {} bytes but LV size is {}",
            logical_volume.name, expected, logical_volume.size_bytes
        )));
    }
    Ok(())
}

fn metadata_error(message: String) -> LvmError {
    LvmError::MetadataParseError { line: 0, message }
}
