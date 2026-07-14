use std::collections::HashSet;

use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection};

use super::ceph_bluefs_repo::{self, CephBluefsAggregate, CephBluefsSuperblockRecord};
use super::ceph_rocksdb_latest_state_repo::{self, CephRocksdbLatestStateRecord};
use super::ceph_rocksdb_repo::{self, CephRocksdbAggregate};
use super::ceph_rocksdb_sst_repo::{self, CephRocksdbSstRecord};
use super::ceph_rocksdb_wal_repo::{self, CephRocksdbWalAggregate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephOsdInventoryRecord {
    pub id: String,
    pub data_source_id: String,
    pub partition_index: Option<u32>,
    pub lvm_vg_uuid: Option<String>,
    pub lvm_vg_name: Option<String>,
    pub lvm_lv_uuid: Option<String>,
    pub lvm_lv_name: Option<String>,
    pub osd_uuid: String,
    pub ceph_fsid: Option<String>,
    pub whoami: Option<u32>,
    pub device_role: String,
    pub device_size: u64,
    pub birth_time_seconds: i64,
    pub birth_time_nanoseconds: u32,
    pub description: String,
    pub is_multi: bool,
    pub selected_epoch: Option<i64>,
    pub valid_label_count: u32,
    pub label_health: String,
    pub osd_key_present: bool,
    pub kv_backend: Option<String>,
    pub bluefs_enabled: Option<bool>,
    pub ceph_version_when_created: Option<String>,
    pub require_osd_release: Option<u32>,
    pub sanitized_metadata_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephOsdLabelReplicaRecord {
    pub inventory_id: String,
    pub position: u64,
    pub device_size: u64,
    pub birth_time_seconds: i64,
    pub birth_time_nanoseconds: u32,
    pub description: String,
    pub is_multi: bool,
    pub epoch: Option<i64>,
    pub is_selected: bool,
    pub struct_version: u8,
    pub struct_compat_version: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct CephRocksdbMetadataSnapshot<'a> {
    pub bluefs: &'a CephBluefsAggregate,
    pub rocksdb: &'a CephRocksdbAggregate,
    pub ssts: &'a [CephRocksdbSstRecord],
    pub wals: &'a CephRocksdbWalAggregate,
    pub latest_state: &'a [CephRocksdbLatestStateRecord],
}

#[derive(Debug, Clone, Copy, Default)]
struct CephAggregateReplacement<'a> {
    bluefs: Option<&'a CephBluefsAggregate>,
    rocksdb: Option<&'a CephRocksdbAggregate>,
    ssts: Option<&'a [CephRocksdbSstRecord]>,
    wals: Option<&'a CephRocksdbWalAggregate>,
    latest_state: Option<&'a [CephRocksdbLatestStateRecord]>,
}

impl<'a> From<CephRocksdbMetadataSnapshot<'a>> for CephAggregateReplacement<'a> {
    fn from(value: CephRocksdbMetadataSnapshot<'a>) -> Self {
        Self {
            bluefs: Some(value.bluefs),
            rocksdb: Some(value.rocksdb),
            ssts: Some(value.ssts),
            wals: Some(value.wals),
            latest_state: Some(value.latest_state),
        }
    }
}

pub struct CephOsdRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephOsdRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn replace_for_data_source(
        &self,
        data_source_id: &str,
        inventory: &[CephOsdInventoryRecord],
        replicas: &[CephOsdLabelReplicaRecord],
    ) -> DbResult<()> {
        self.replace_aggregate(
            data_source_id,
            inventory,
            replicas,
            CephAggregateReplacement::default(),
        )
    }

    pub fn replace_for_data_source_with_bluefs(
        &self,
        data_source_id: &str,
        inventory: &[CephOsdInventoryRecord],
        replicas: &[CephOsdLabelReplicaRecord],
        bluefs: Option<&CephBluefsAggregate>,
    ) -> DbResult<()> {
        self.replace_aggregate(
            data_source_id,
            inventory,
            replicas,
            CephAggregateReplacement {
                bluefs,
                ..CephAggregateReplacement::default()
            },
        )
    }

    pub fn replace_for_data_source_with_rocksdb_metadata(
        &self,
        data_source_id: &str,
        inventory: &[CephOsdInventoryRecord],
        replicas: &[CephOsdLabelReplicaRecord],
        metadata: CephRocksdbMetadataSnapshot<'_>,
    ) -> DbResult<()> {
        self.replace_aggregate(data_source_id, inventory, replicas, metadata.into())
    }

    fn replace_aggregate(
        &self,
        data_source_id: &str,
        inventory: &[CephOsdInventoryRecord],
        replicas: &[CephOsdLabelReplicaRecord],
        metadata: CephAggregateReplacement<'_>,
    ) -> DbResult<()> {
        let CephAggregateReplacement {
            bluefs,
            rocksdb,
            ssts,
            wals,
            latest_state,
        } = metadata;
        if rocksdb.is_some() != ssts.is_some()
            || rocksdb.is_some() != wals.is_some()
            || rocksdb.is_some() != latest_state.is_some()
        {
            return Err(DbError::System(
                "RocksDB manifest, complete SST inventory, WAL inventory, and latest-state summaries must be replaced together"
                    .to_string(),
            ));
        }
        validate_replacement(data_source_id, inventory, replicas)?;
        if let Some(records) = bluefs {
            validate_bluefs_binding(data_source_id, inventory, &records.superblock)?;
            ceph_bluefs_repo::validate_replacement(records)?;
        }
        if let Some(records) = rocksdb {
            validate_rocksdb_binding(data_source_id, inventory, bluefs, records)?;
            ceph_rocksdb_repo::validate_replacement(records)?;
        }
        if let (Some(rocksdb), Some(records)) = (rocksdb, ssts) {
            ceph_rocksdb_sst_repo::validate_replacement(rocksdb, records)?;
        }
        if let (Some(bluefs), Some(rocksdb), Some(wals)) = (bluefs, rocksdb, wals) {
            ceph_rocksdb_wal_repo::validate_replacement(bluefs, rocksdb, wals)?;
        }
        if let (Some(rocksdb), Some(latest_state)) = (rocksdb, latest_state) {
            ceph_rocksdb_latest_state_repo::validate_replacement(rocksdb, latest_state)?;
        }
        let tx = self.conn.unchecked_transaction()?;
        replace_for_data_source_on(&tx, data_source_id, inventory, replicas)?;
        if let Some(records) = bluefs {
            ceph_bluefs_repo::replace_for_inventory_on(&tx, records)?;
        }
        if let (Some(records), Some(ssts), Some(wals), Some(latest_state)) =
            (rocksdb, ssts, wals, latest_state)
        {
            ceph_rocksdb_repo::replace_for_inventory_on(&tx, records)?;
            ceph_rocksdb_sst_repo::replace_for_inventory_on(
                &tx,
                &records.manifest.inventory_id,
                ssts,
            )?;
            ceph_rocksdb_wal_repo::replace_for_inventory_on(
                &tx,
                &records.manifest.inventory_id,
                wals,
            )?;
            ceph_rocksdb_latest_state_repo::replace_on(
                &tx,
                &records.manifest.inventory_id,
                latest_state,
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn find_by_data_source(
        &self,
        data_source_id: &str,
    ) -> DbResult<Vec<CephOsdInventoryRecord>> {
        let mut statement = self.conn.prepare(
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

    pub fn find_label_replicas(
        &self,
        inventory_id: &str,
    ) -> DbResult<Vec<CephOsdLabelReplicaRecord>> {
        let mut statement = self.conn.prepare(
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
}

fn replace_for_data_source_on(
    conn: &Connection,
    data_source_id: &str,
    inventory: &[CephOsdInventoryRecord],
    replicas: &[CephOsdLabelReplicaRecord],
) -> DbResult<()> {
    conn.execute(
        "DELETE FROM ceph_osd_inventory WHERE data_source_id = ?1",
        params![data_source_id],
    )?;

    let mut inventory_statement = conn.prepare_cached(
        "INSERT INTO ceph_osd_inventory (
            id, data_source_id, partition_index, lvm_vg_uuid, lvm_vg_name,
            lvm_lv_uuid, lvm_lv_name, osd_uuid, ceph_fsid, whoami, device_role,
            device_size, birth_time_seconds, birth_time_nanoseconds, description,
            is_multi, selected_epoch, valid_label_count, label_health, osd_key_present,
            kv_backend, bluefs_enabled, ceph_version_when_created, require_osd_release,
            sanitized_metadata_json
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25
         )",
    )?;
    for record in inventory {
        inventory_statement.execute(params![
            record.id,
            record.data_source_id,
            record.partition_index,
            record.lvm_vg_uuid,
            record.lvm_vg_name,
            record.lvm_lv_uuid,
            record.lvm_lv_name,
            record.osd_uuid,
            record.ceph_fsid,
            record.whoami,
            record.device_role,
            record.device_size,
            record.birth_time_seconds,
            record.birth_time_nanoseconds,
            record.description,
            record.is_multi,
            record.selected_epoch,
            record.valid_label_count,
            record.label_health,
            record.osd_key_present,
            record.kv_backend,
            record.bluefs_enabled,
            record.ceph_version_when_created,
            record.require_osd_release,
            record.sanitized_metadata_json,
        ])?;
    }
    drop(inventory_statement);

    let mut replica_statement = conn.prepare_cached(
        "INSERT INTO ceph_osd_label_replicas (
            inventory_id, position, device_size, birth_time_seconds,
            birth_time_nanoseconds, description, is_multi, epoch, is_selected,
            struct_version, struct_compat_version
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for record in replicas {
        replica_statement.execute(params![
            record.inventory_id,
            record.position,
            record.device_size,
            record.birth_time_seconds,
            record.birth_time_nanoseconds,
            record.description,
            record.is_multi,
            record.epoch,
            record.is_selected,
            record.struct_version,
            record.struct_compat_version,
        ])?;
    }
    Ok(())
}

fn validate_bluefs_binding(
    data_source_id: &str,
    inventory: &[CephOsdInventoryRecord],
    superblock: &CephBluefsSuperblockRecord,
) -> DbResult<()> {
    if superblock.data_source_id != data_source_id {
        return Err(DbError::System(
            "BlueFS superblock belongs to a different data source".to_string(),
        ));
    }
    let osd = inventory
        .iter()
        .find(|record| record.id == superblock.inventory_id)
        .ok_or_else(|| {
            DbError::System(format!(
                "BlueFS superblock references unknown OSD inventory: {}",
                superblock.inventory_id
            ))
        })?;
    if osd.osd_uuid != superblock.osd_uuid {
        return Err(DbError::System(
            "BlueFS superblock OSD UUID does not match its inventory".to_string(),
        ));
    }
    Ok(())
}

fn validate_rocksdb_binding(
    data_source_id: &str,
    inventory: &[CephOsdInventoryRecord],
    bluefs: Option<&CephBluefsAggregate>,
    rocksdb: &CephRocksdbAggregate,
) -> DbResult<()> {
    let manifest = &rocksdb.manifest;
    if manifest.data_source_id != data_source_id {
        return Err(DbError::System(
            "RocksDB manifest belongs to a different data source".to_string(),
        ));
    }
    if !inventory
        .iter()
        .any(|record| record.id == manifest.inventory_id)
    {
        return Err(DbError::System(format!(
            "RocksDB manifest references unknown OSD inventory: {}",
            manifest.inventory_id
        )));
    }
    let bluefs = bluefs.ok_or_else(|| {
        DbError::System("RocksDB inventory requires a BlueFS replay snapshot".to_string())
    })?;
    if bluefs.superblock.inventory_id != manifest.inventory_id
        || bluefs.replay.replay.inventory_id != manifest.inventory_id
    {
        return Err(DbError::System(
            "RocksDB inventory belongs to another BlueFS snapshot".to_string(),
        ));
    }
    Ok(())
}

fn validate_replacement(
    data_source_id: &str,
    inventory: &[CephOsdInventoryRecord],
    replicas: &[CephOsdLabelReplicaRecord],
) -> DbResult<()> {
    let inventory_ids = inventory
        .iter()
        .map(|record| {
            if record.data_source_id != data_source_id {
                return Err(DbError::System(format!(
                    "Ceph OSD inventory {} belongs to a different data source",
                    record.id
                )));
            }
            Ok(record.id.as_str())
        })
        .collect::<DbResult<HashSet<_>>>()?;

    if let Some(replica) = replicas
        .iter()
        .find(|replica| !inventory_ids.contains(replica.inventory_id.as_str()))
    {
        return Err(DbError::System(format!(
            "Ceph label replica references unknown inventory: {}",
            replica.inventory_id
        )));
    }
    Ok(())
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
