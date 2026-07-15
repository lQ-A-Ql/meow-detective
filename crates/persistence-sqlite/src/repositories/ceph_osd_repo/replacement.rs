use std::time::Instant;

use rusqlite::{params, Connection};

use crate::connection::{DbError, DbResult};

use super::super::ceph_bluefs_repo::{self, CephBluefsAggregate};
use super::super::ceph_bluestore_semantic_repo::{self, CephBluestoreSemanticAggregate};
use super::super::ceph_rocksdb_latest_state_repo::{self, CephRocksdbLatestStateRecord};
use super::super::ceph_rocksdb_repo::{self, CephRocksdbAggregate};
use super::super::ceph_rocksdb_sst_repo::{self, CephRocksdbSstRecord};
use super::super::ceph_rocksdb_wal_repo::{self, CephRocksdbWalAggregate};
use super::validation;
use super::{CephOsdInventoryRecord, CephOsdLabelReplicaRecord, CephRocksdbMetadataSnapshot};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct CephAggregateReplacement<'a> {
    bluefs: Option<&'a CephBluefsAggregate>,
    rocksdb: Option<&'a CephRocksdbAggregate>,
    ssts: Option<&'a [CephRocksdbSstRecord]>,
    wals: Option<&'a CephRocksdbWalAggregate>,
    latest_state: Option<&'a [CephRocksdbLatestStateRecord]>,
    semantic: Option<&'a CephBluestoreSemanticAggregate>,
}

impl<'a> CephAggregateReplacement<'a> {
    pub(super) fn with_bluefs(bluefs: Option<&'a CephBluefsAggregate>) -> Self {
        Self {
            bluefs,
            ..Self::default()
        }
    }
}

impl<'a> From<CephRocksdbMetadataSnapshot<'a>> for CephAggregateReplacement<'a> {
    fn from(value: CephRocksdbMetadataSnapshot<'a>) -> Self {
        Self {
            bluefs: Some(value.bluefs),
            rocksdb: Some(value.rocksdb),
            ssts: Some(value.ssts),
            wals: Some(value.wals),
            latest_state: Some(value.latest_state),
            semantic: Some(value.semantic),
        }
    }
}

pub(super) fn replace_aggregate(
    conn: &Connection,
    data_source_id: &str,
    inventory: &[CephOsdInventoryRecord],
    replicas: &[CephOsdLabelReplicaRecord],
    metadata: CephAggregateReplacement<'_>,
) -> DbResult<()> {
    let total_started = Instant::now();
    let CephAggregateReplacement {
        bluefs,
        rocksdb,
        ssts,
        wals,
        latest_state,
        semantic,
    } = metadata;
    if rocksdb.is_some() != ssts.is_some()
        || rocksdb.is_some() != wals.is_some()
        || rocksdb.is_some() != latest_state.is_some()
        || rocksdb.is_some() != semantic.is_some()
    {
        return Err(DbError::System(
            "RocksDB manifest, SST/WAL inventory, latest-state summaries, and BlueStore semantics must be replaced together"
                .to_string(),
        ));
    }
    validation::validate_replacement(data_source_id, inventory, replicas)?;
    if let Some(records) = bluefs {
        validation::validate_bluefs_binding(data_source_id, inventory, &records.superblock)?;
        ceph_bluefs_repo::validate_replacement(records)?;
    }
    if let Some(records) = rocksdb {
        validation::validate_rocksdb_binding(data_source_id, inventory, bluefs, records)?;
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
    if let (Some(rocksdb), Some(latest_state), Some(semantic)) = (rocksdb, latest_state, semantic) {
        validation::validate_semantic_binding(inventory, rocksdb, latest_state, semantic)?;
        ceph_bluestore_semantic_repo::validate_replacement(semantic)?;
    }
    let validation_elapsed_ms = total_started.elapsed().as_millis();
    let tx = conn.unchecked_transaction()?;
    let write_started = Instant::now();
    replace_for_data_source_on(&tx, data_source_id, inventory, replicas)?;
    if let Some(records) = bluefs {
        ceph_bluefs_repo::replace_for_inventory_on(&tx, records)?;
    }
    if let (Some(records), Some(ssts), Some(wals), Some(latest_state), Some(semantic)) =
        (rocksdb, ssts, wals, latest_state, semantic)
    {
        ceph_rocksdb_repo::replace_for_inventory_on(&tx, records)?;
        ceph_rocksdb_sst_repo::replace_for_inventory_on(&tx, &records.manifest.inventory_id, ssts)?;
        ceph_rocksdb_wal_repo::replace_for_inventory_on(&tx, &records.manifest.inventory_id, wals)?;
        ceph_rocksdb_latest_state_repo::replace_on(
            &tx,
            &records.manifest.inventory_id,
            latest_state,
        )?;
        ceph_bluestore_semantic_repo::replace_validated_for_inventory_on(&tx, semantic)?;
    }
    let write_elapsed_ms = write_started.elapsed().as_millis();
    let commit_started = Instant::now();
    tx.commit()?;
    let commit_elapsed_ms = commit_started.elapsed().as_millis();
    tracing::info!(
        data_source_id,
        inventory_count = inventory.len(),
        semantic_object_rows = semantic.map_or(0, |value| value.objects.len()),
        semantic_checksum_rows = semantic.map_or(0, |value| value.checksum_chunks.len()),
        validation_elapsed_ms,
        write_elapsed_ms,
        commit_elapsed_ms,
        total_elapsed_ms = total_started.elapsed().as_millis(),
        "Replaced transactional Ceph metadata aggregate"
    );
    Ok(())
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
