use std::collections::HashSet;

use crate::connection::{DbError, DbResult};

use super::super::ceph_bluefs_repo::{CephBluefsAggregate, CephBluefsSuperblockRecord};
use super::super::ceph_bluestore_omap_repo::{self, CephBluestoreOmapAggregate};
use super::super::ceph_bluestore_semantic_repo::{self, CephBluestoreSemanticAggregate};
use super::super::ceph_osd_device_binding_repo::{self, CephOsdDeviceBindingAggregate};
use super::super::ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord;
use super::super::ceph_rocksdb_repo::CephRocksdbAggregate;
use super::{CephOsdInventoryRecord, CephOsdLabelReplicaRecord};

pub(super) fn validate_omap_binding(
    data_source_id: &str,
    inventory: &[CephOsdInventoryRecord],
    rocksdb: &CephRocksdbAggregate,
    latest_state: &[CephRocksdbLatestStateRecord],
    semantic: &CephBluestoreSemanticAggregate,
    omap: &CephBluestoreOmapAggregate,
) -> DbResult<()> {
    let osd = inventory
        .iter()
        .find(|record| record.id == rocksdb.manifest.inventory_id)
        .ok_or_else(|| {
            DbError::System("BlueStore OMAP snapshot has no OSD inventory".to_string())
        })?;
    if osd.data_source_id != data_source_id || omap.scan.data_source_id != data_source_id {
        return Err(DbError::System(
            "BlueStore OMAP snapshot crosses data-source ownership".to_string(),
        ));
    }
    ceph_bluestore_omap_repo::validate_recovery_binding(rocksdb, latest_state, semantic, omap)
}

pub(super) fn validate_semantic_binding(
    inventory: &[CephOsdInventoryRecord],
    rocksdb: &CephRocksdbAggregate,
    latest_state: &[CephRocksdbLatestStateRecord],
    semantic: &CephBluestoreSemanticAggregate,
) -> DbResult<()> {
    ceph_bluestore_semantic_repo::validation::validate_recovery_binding(
        rocksdb,
        latest_state,
        semantic,
    )?;
    let inventory_id = rocksdb.manifest.inventory_id.as_str();
    let device_size = inventory
        .iter()
        .find(|record| record.id == inventory_id)
        .map(|record| record.device_size)
        .ok_or_else(|| {
            DbError::System("BlueStore semantic snapshot has no OSD inventory".to_string())
        })?;
    ceph_bluestore_semantic_repo::validation::validate_device_bounds(semantic, device_size)
}

pub(super) fn validate_bluefs_binding(
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

pub(super) fn validate_rocksdb_binding(
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

pub(super) fn validate_replacement(
    data_source_id: &str,
    inventory: &[CephOsdInventoryRecord],
    replicas: &[CephOsdLabelReplicaRecord],
    device_bindings: &[CephOsdDeviceBindingAggregate],
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

    ceph_osd_device_binding_repo::validate_replacement(
        data_source_id,
        &inventory_ids,
        device_bindings,
    )?;

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
