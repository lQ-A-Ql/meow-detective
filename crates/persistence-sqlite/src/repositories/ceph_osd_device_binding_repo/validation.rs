use std::collections::HashSet;

use crate::connection::{DbError, DbResult};

use super::CephOsdDeviceBindingAggregate;

pub(super) fn validate_replacement(
    data_source_id: &str,
    inventory_ids: &HashSet<&str>,
    bindings: &[CephOsdDeviceBindingAggregate],
) -> DbResult<()> {
    let mut bound_inventory_ids = HashSet::new();
    for binding in bindings {
        validate_device(data_source_id, inventory_ids, binding)?;
        if !bound_inventory_ids.insert(binding.device.inventory_id.as_str()) {
            return Err(DbError::System(format!(
                "duplicate Ceph OSD device binding: {}",
                binding.device.inventory_id
            )));
        }
    }
    Ok(())
}

fn validate_device(
    data_source_id: &str,
    inventory_ids: &HashSet<&str>,
    binding: &CephOsdDeviceBindingAggregate,
) -> DbResult<()> {
    let device = &binding.device;
    if device.data_source_id != data_source_id {
        return Err(DbError::System(
            "Ceph OSD device binding crosses data-source ownership".to_string(),
        ));
    }
    if !inventory_ids.contains(device.inventory_id.as_str()) {
        return Err(DbError::System(format!(
            "Ceph OSD device binding references unknown inventory: {}",
            device.inventory_id
        )));
    }
    validate_text("source path", &device.source_path)?;
    validate_text("canonical source path", &device.canonical_source_path)?;
    validate_source_kind(&device.source_kind)?;
    validate_text("LVM VG UUID", &device.lvm_vg_uuid)?;
    validate_text("LVM VG name", &device.lvm_vg_name)?;
    validate_text("LVM LV UUID", &device.lvm_lv_uuid)?;
    validate_text("LVM LV name", &device.lvm_lv_name)?;
    validate_sqlite_u64("device size", device.device_size, false)?;
    if binding.physical_volumes.is_empty() {
        return Err(DbError::System(
            "Ceph OSD device binding has no physical volumes".to_string(),
        ));
    }

    let mut pv_uuids = HashSet::new();
    for (index, pv) in binding.physical_volumes.iter().enumerate() {
        if pv.inventory_id != device.inventory_id {
            return Err(DbError::System(
                "Ceph OSD PV binding references another inventory".to_string(),
            ));
        }
        if usize::try_from(pv.ordinal).ok() != Some(index) {
            return Err(DbError::System(
                "Ceph OSD PV binding ordinals are not contiguous".to_string(),
            ));
        }
        validate_text("PV source path", &pv.source_path)?;
        validate_text("PV canonical source path", &pv.canonical_source_path)?;
        validate_source_kind(&pv.source_kind)?;
        validate_sqlite_u64("PV offset", pv.pv_offset, true)?;
        validate_text("PV UUID", &pv.pv_uuid)?;
        if pv.pv_name.as_deref().is_some_and(str::is_empty) {
            return Err(DbError::System(
                "Ceph OSD PV binding has an empty PV name".to_string(),
            ));
        }
        let normalized_uuid = normalize_lvm_uuid(&pv.pv_uuid);
        if normalized_uuid.is_empty() || !pv_uuids.insert(normalized_uuid) {
            return Err(DbError::System(
                "Ceph OSD PV binding UUIDs are empty or duplicated".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_text(label: &str, value: &str) -> DbResult<()> {
    if value.is_empty() || value.contains('\0') {
        return Err(DbError::System(format!(
            "Ceph OSD device binding has invalid {label}"
        )));
    }
    Ok(())
}

fn validate_source_kind(value: &str) -> DbResult<()> {
    if matches!(value, "e01" | "raw") {
        Ok(())
    } else {
        Err(DbError::System(format!(
            "unsupported Ceph OSD device binding source kind: {value}"
        )))
    }
}

fn validate_sqlite_u64(label: &str, value: u64, allow_zero: bool) -> DbResult<()> {
    if (!allow_zero && value == 0) || value > i64::MAX as u64 {
        return Err(DbError::System(format!(
            "Ceph OSD device binding {label} is outside SQLite integer range"
        )));
    }
    Ok(())
}

fn normalize_lvm_uuid(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| *character != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}
