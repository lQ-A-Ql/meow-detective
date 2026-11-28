use domain::structure::{
    CapabilityKind, EvidenceRequirement, MetadataFailure, SourceRevisionRef, TargetFailure,
};
use persistence_sqlite::repositories::{
    ceph_osd_repo::{CephOsdInventoryRecord, CephOsdRepo},
    ceph_rocksdb_latest_state_repo::{
        self, CephRocksdbLatestStateRecord, CephRocksdbLatestStateRepo,
    },
    ceph_rocksdb_repo::{
        CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbLiveSstRecord,
        CephRocksdbManifestRecord, CephRocksdbRepo,
    },
};
use rusqlite::Connection;
use serde_json::{json, Value};

use super::{check_cancel, corrupt, missing, ExecutionContext, RuntimeError};

pub(super) fn analyze(
    connection: &Connection,
    context: &ExecutionContext<'_>,
    source: &SourceRevisionRef,
) -> Result<Value, RuntimeError> {
    let inventories = CephOsdRepo::new(connection).find_by_data_source(&source.source_id)?;
    if inventories.is_empty() {
        return Err(RuntimeError::Target(TargetFailure::MissingCapabilities {
            capabilities: [CapabilityKind::MemberMetadata].into_iter().collect(),
        }));
    }
    let rocksdb = CephRocksdbRepo::new(connection);
    let mut output = Vec::with_capacity(inventories.len());
    for inventory in &inventories {
        check_cancel(context)?;
        if inventory.data_source_id != source.source_id {
            return Err(corrupt(MetadataFailure::InconsistentReferences));
        }
        if inventory
            .kv_backend
            .as_deref()
            .is_some_and(|backend| backend != "rocksdb")
        {
            return Err(RuntimeError::Target(TargetFailure::UnsupportedFeature {
                capability: CapabilityKind::KvInventory,
            }));
        }
        let aggregate = rocksdb
            .find_aggregate(&inventory.id)?
            .ok_or_else(|| missing(EvidenceRequirement::NativeVersionWitness))?;
        require_aggregate_owner(&aggregate, inventory, source)?;
        let latest = CephRocksdbLatestStateRepo::new(connection).find(&inventory.id)?;
        if !latest.is_empty() {
            ceph_rocksdb_latest_state_repo::validate_replacement(&aggregate, &latest)?;
        }
        output.push(json!({
            "inventory": inventory_summary(inventory),
            "manifest": manifest_summary(&aggregate.manifest),
            "columnFamilies": aggregate.column_families.iter().map(column_family_summary).collect::<Vec<_>>(),
            "liveSsts": aggregate.live_ssts.iter().map(live_sst_summary).collect::<Vec<_>>(),
            "latestState": latest.iter().map(latest_state_summary).collect::<Vec<_>>(),
        }));
    }
    Ok(json!({
        "sourceRevision": source,
        "inventoryCount": inventories.len(),
        "osds": output,
    }))
}

fn require_aggregate_owner(
    aggregate: &CephRocksdbAggregate,
    inventory: &CephOsdInventoryRecord,
    source: &SourceRevisionRef,
) -> Result<(), RuntimeError> {
    if aggregate.manifest.inventory_id != inventory.id
        || aggregate.manifest.data_source_id != source.source_id
        || aggregate
            .column_families
            .iter()
            .any(|family| family.inventory_id != inventory.id)
        || aggregate
            .live_ssts
            .iter()
            .any(|sst| sst.inventory_id != inventory.id)
    {
        return Err(corrupt(MetadataFailure::InconsistentReferences));
    }
    if aggregate.column_families.is_empty() {
        return Err(RuntimeError::Target(TargetFailure::MissingCapabilities {
            capabilities: [CapabilityKind::KvInventory].into_iter().collect(),
        }));
    }
    Ok(())
}

fn inventory_summary(record: &CephOsdInventoryRecord) -> Value {
    json!({
        "inventoryId": record.id,
        "partitionIndex": record.partition_index,
        "lvmVgUuid": record.lvm_vg_uuid,
        "lvmLvUuid": record.lvm_lv_uuid,
        "osdUuid": record.osd_uuid,
        "cephFsid": record.ceph_fsid,
        "whoami": record.whoami,
        "deviceRole": record.device_role,
        "deviceSize": record.device_size,
        "birthTimeSeconds": record.birth_time_seconds,
        "birthTimeNanoseconds": record.birth_time_nanoseconds,
        "isMulti": record.is_multi,
        "selectedEpoch": record.selected_epoch,
        "validLabelCount": record.valid_label_count,
        "labelHealth": record.label_health,
        "osdKeyPresent": record.osd_key_present,
        "kvBackend": record.kv_backend,
        "bluefsEnabled": record.bluefs_enabled,
        "cephVersionWhenCreated": record.ceph_version_when_created,
        "requireOsdRelease": record.require_osd_release,
    })
}

fn manifest_summary(record: &CephRocksdbManifestRecord) -> Value {
    json!({
        "manifestFileNumber": record.manifest_file_number,
        "manifestFileSize": record.manifest_file_size,
        "identityUuid": record.identity_uuid,
        "logicalEditCount": record.logical_edit_count,
        "comparatorName": record.comparator_name,
        "lastSequence": record.last_sequence,
        "nextFileNumber": record.next_file_number,
        "logNumber": record.log_number,
        "previousLogNumber": record.prev_log_number,
        "maxColumnFamilyId": record.max_column_family_id,
        "minLogNumberToKeep": record.min_log_number_to_keep,
    })
}

fn column_family_summary(record: &CephRocksdbColumnFamilyRecord) -> Value {
    json!({
        "columnFamilyId": record.column_family_id,
        "name": record.name,
        "comparatorName": record.comparator_name,
        "dropped": record.dropped,
        "logNumber": record.log_number,
    })
}

fn live_sst_summary(record: &CephRocksdbLiveSstRecord) -> Value {
    json!({
        "columnFamilyId": record.column_family_id,
        "level": record.level,
        "fileNumber": record.file_number,
        "pathId": record.path_id,
        "format": record.format,
        "fileSize": record.file_size,
        "smallestSequence": record.smallest_sequence,
        "largestSequence": record.largest_sequence,
        "smallestInternalKeyLength": record.smallest_internal_key_length,
        "largestInternalKeyLength": record.largest_internal_key_length,
    })
}

fn latest_state_summary(record: &CephRocksdbLatestStateRecord) -> Value {
    json!({
        "columnFamilyId": record.column_family_id,
        "columnFamilyName": record.column_family_name,
        "schemaVersion": record.schema_version,
        "shardingSha256": record.sharding_sha256,
        "pointMutationCount": record.point_mutation_count,
        "rangeMutationCount": record.range_mutation_count,
        "latestValueCount": record.latest_value_count,
        "deletedKeyCount": record.deleted_key_count,
        "mergeResolvedCount": record.merge_resolved_count,
        "mergeOperandCount": record.merge_operand_count,
        "smallestSequence": record.smallest_sequence,
        "largestSequence": record.largest_sequence,
        "pointSha256": record.point_sha256,
        "rangeSha256": record.range_sha256,
        "latestStateSha256": record.latest_state_sha256,
        "scanComplete": record.scan_complete,
    })
}
