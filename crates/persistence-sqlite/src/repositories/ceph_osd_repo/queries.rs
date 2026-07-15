use rusqlite::{params, Connection};

use crate::connection::DbResult;

use super::{CephOsdInventoryRecord, CephOsdLabelReplicaRecord};

pub(super) fn find_by_data_source(
    conn: &Connection,
    data_source_id: &str,
) -> DbResult<Vec<CephOsdInventoryRecord>> {
    let mut statement = conn.prepare(
        "SELECT id, data_source_id, partition_index, lvm_vg_uuid, lvm_vg_name,
                lvm_lv_uuid, lvm_lv_name, osd_uuid, ceph_fsid, whoami, device_role,
                device_size, birth_time_seconds, birth_time_nanoseconds, description,
                is_multi, selected_epoch, valid_label_count, label_health, osd_key_present,
                kv_backend, bluefs_enabled, ceph_version_when_created, require_osd_release,
                sanitized_metadata_json
         FROM ceph_osd_inventory
         WHERE data_source_id = ?1
         ORDER BY whoami IS NULL, whoami, osd_uuid, id",
    )?;
    let rows = statement.query_map(params![data_source_id], map_inventory)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn find_label_replicas(
    conn: &Connection,
    inventory_id: &str,
) -> DbResult<Vec<CephOsdLabelReplicaRecord>> {
    let mut statement = conn.prepare(
        "SELECT inventory_id, position, device_size, birth_time_seconds,
                birth_time_nanoseconds, description, is_multi, epoch, is_selected,
                struct_version, struct_compat_version
         FROM ceph_osd_label_replicas
         WHERE inventory_id = ?1
         ORDER BY position",
    )?;
    let rows = statement.query_map(params![inventory_id], map_replica)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn map_inventory(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephOsdInventoryRecord> {
    Ok(CephOsdInventoryRecord {
        id: row.get(0)?,
        data_source_id: row.get(1)?,
        partition_index: row.get(2)?,
        lvm_vg_uuid: row.get(3)?,
        lvm_vg_name: row.get(4)?,
        lvm_lv_uuid: row.get(5)?,
        lvm_lv_name: row.get(6)?,
        osd_uuid: row.get(7)?,
        ceph_fsid: row.get(8)?,
        whoami: row.get(9)?,
        device_role: row.get(10)?,
        device_size: row.get(11)?,
        birth_time_seconds: row.get(12)?,
        birth_time_nanoseconds: row.get(13)?,
        description: row.get(14)?,
        is_multi: row.get(15)?,
        selected_epoch: row.get(16)?,
        valid_label_count: row.get(17)?,
        label_health: row.get(18)?,
        osd_key_present: row.get(19)?,
        kv_backend: row.get(20)?,
        bluefs_enabled: row.get(21)?,
        ceph_version_when_created: row.get(22)?,
        require_osd_release: row.get(23)?,
        sanitized_metadata_json: row.get(24)?,
    })
}

fn map_replica(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephOsdLabelReplicaRecord> {
    Ok(CephOsdLabelReplicaRecord {
        inventory_id: row.get(0)?,
        position: row.get(1)?,
        device_size: row.get(2)?,
        birth_time_seconds: row.get(3)?,
        birth_time_nanoseconds: row.get(4)?,
        description: row.get(5)?,
        is_multi: row.get(6)?,
        epoch: row.get(7)?,
        is_selected: row.get(8)?,
        struct_version: row.get(9)?,
        struct_compat_version: row.get(10)?,
    })
}
