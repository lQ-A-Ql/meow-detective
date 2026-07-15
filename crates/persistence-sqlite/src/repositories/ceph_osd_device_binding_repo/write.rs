use rusqlite::{params, Connection};

use crate::connection::DbResult;

use super::CephOsdDeviceBindingAggregate;

pub(super) fn replace_for_data_source_on(
    conn: &Connection,
    bindings: &[CephOsdDeviceBindingAggregate],
) -> DbResult<()> {
    let mut device_statement = conn.prepare_cached(
        "INSERT INTO ceph_osd_device_bindings (
            inventory_id, data_source_id, source_path, canonical_source_path,
            source_kind, lvm_vg_uuid, lvm_vg_name, lvm_lv_uuid, lvm_lv_name,
            device_size
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    let mut pv_statement = conn.prepare_cached(
        "INSERT INTO ceph_osd_device_binding_pvs (
            inventory_id, ordinal, source_path, canonical_source_path, source_kind,
            pv_offset, pv_uuid, pv_name
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;

    for binding in bindings {
        let device = &binding.device;
        device_statement.execute(params![
            device.inventory_id,
            device.data_source_id,
            device.source_path,
            device.canonical_source_path,
            device.source_kind,
            device.lvm_vg_uuid,
            device.lvm_vg_name,
            device.lvm_lv_uuid,
            device.lvm_lv_name,
            device.device_size,
        ])?;
        for pv in &binding.physical_volumes {
            pv_statement.execute(params![
                pv.inventory_id,
                pv.ordinal,
                pv.source_path,
                pv.canonical_source_path,
                pv.source_kind,
                pv.pv_offset,
                pv.pv_uuid,
                pv.pv_name,
            ])?;
        }
    }
    Ok(())
}
