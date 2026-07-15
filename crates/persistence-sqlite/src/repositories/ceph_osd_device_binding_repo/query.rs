use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::DbResult;

use super::{
    CephOsdDeviceBindingAggregate, CephOsdDeviceBindingRecord, CephOsdPvBindingRecord,
    CephOsdRegisteredSourceIdentity, CephOsdSourceBoundDevice,
};

pub(super) fn find_source_bound_device(
    conn: &Connection,
    data_source_id: &str,
    inventory_id: &str,
) -> DbResult<Option<CephOsdSourceBoundDevice>> {
    let source = conn
        .query_row(
            "SELECT id, source_path, canonical_source_path, kind
             FROM data_sources
             WHERE id = ?1",
            params![data_source_id],
            |row| {
                Ok(CephOsdRegisteredSourceIdentity {
                    data_source_id: row.get(0)?,
                    source_path: row.get(1)?,
                    canonical_source_path: row.get(2)?,
                    source_kind: row.get(3)?,
                })
            },
        )
        .optional()?;
    let Some(source) = source else {
        return Ok(None);
    };

    let device = conn
        .query_row(
            "SELECT inventory_id, data_source_id, source_path, canonical_source_path,
                    source_kind, lvm_vg_uuid, lvm_vg_name, lvm_lv_uuid, lvm_lv_name,
                    device_size
             FROM ceph_osd_device_bindings
             WHERE inventory_id = ?1 AND data_source_id = ?2",
            params![inventory_id, data_source_id],
            map_device,
        )
        .optional()?;
    let Some(device) = device else {
        return Ok(None);
    };

    let mut statement = conn.prepare(
        "SELECT inventory_id, ordinal, source_path, canonical_source_path, source_kind,
                pv_offset, pv_uuid, pv_name
         FROM ceph_osd_device_binding_pvs
         WHERE inventory_id = ?1
         ORDER BY ordinal",
    )?;
    let physical_volumes = statement
        .query_map(params![inventory_id], map_physical_volume)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(CephOsdSourceBoundDevice {
        source,
        binding: CephOsdDeviceBindingAggregate {
            device,
            physical_volumes,
        },
    }))
}

fn map_device(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephOsdDeviceBindingRecord> {
    Ok(CephOsdDeviceBindingRecord {
        inventory_id: row.get(0)?,
        data_source_id: row.get(1)?,
        source_path: row.get(2)?,
        canonical_source_path: row.get(3)?,
        source_kind: row.get(4)?,
        lvm_vg_uuid: row.get(5)?,
        lvm_vg_name: row.get(6)?,
        lvm_lv_uuid: row.get(7)?,
        lvm_lv_name: row.get(8)?,
        device_size: row.get(9)?,
    })
}

fn map_physical_volume(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephOsdPvBindingRecord> {
    Ok(CephOsdPvBindingRecord {
        inventory_id: row.get(0)?,
        ordinal: row.get(1)?,
        source_path: row.get(2)?,
        canonical_source_path: row.get(3)?,
        source_kind: row.get(4)?,
        pv_offset: row.get(5)?,
        pv_uuid: row.get(6)?,
        pv_name: row.get(7)?,
    })
}
