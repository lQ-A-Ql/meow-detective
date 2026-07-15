use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use persistence_sqlite::repositories::ceph_bluefs_repo::CephBluefsAggregate;
use persistence_sqlite::repositories::ceph_bluestore_omap_repo::CephBluestoreOmapAggregate;
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::CephBluestoreSemanticAggregate;
use persistence_sqlite::repositories::ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord;
use persistence_sqlite::repositories::ceph_rocksdb_repo::CephRocksdbAggregate;
use persistence_sqlite::repositories::ceph_rocksdb_sst_repo::CephRocksdbSstRecord;
use persistence_sqlite::repositories::ceph_rocksdb_wal_repo::CephRocksdbWalAggregate;

use super::context::ImportJobContext;

pub(super) struct CephMetadataAggregate {
    pub(super) bluefs: CephBluefsAggregate,
    pub(super) rocksdb: CephRocksdbAggregate,
    pub(super) ssts: Vec<CephRocksdbSstRecord>,
    pub(super) wals: CephRocksdbWalAggregate,
    pub(super) latest_state: Vec<CephRocksdbLatestStateRecord>,
    pub(super) semantic: CephBluestoreSemanticAggregate,
    pub(super) omap: CephBluestoreOmapAggregate,
}

struct CephMetadataAuditTotals {
    data_block_count: u64,
    entry_count: u64,
    data_bytes: u64,
    wal_record_count: u64,
    wal_mutation_count: u64,
    latest_point_mutation_count: u64,
    latest_range_mutation_count: u64,
    latest_value_count: u64,
    latest_deleted_key_count: u64,
}

pub(super) fn audit_bluefs_inventory(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    records: &CephMetadataAggregate,
) {
    let Some(data_block_count) = checked_sst_sum(&records.ssts, |record| record.data_block_count)
    else {
        audit_sum_overflow(data_source, "data block count");
        return;
    };
    let Some(entry_count) = checked_sst_sum(&records.ssts, |record| record.entry_count) else {
        audit_sum_overflow(data_source, "entry count");
        return;
    };
    let Some(data_bytes) = checked_sst_sum(&records.ssts, |record| record.data_size) else {
        audit_sum_overflow(data_source, "data bytes");
        return;
    };
    let Some(wal_record_count) = checked_wal_sum(&records.wals, |record| {
        u64::from(record.logical_record_count)
    }) else {
        audit_sum_overflow(data_source, "WAL logical record count");
        return;
    };
    let Some(wal_mutation_count) = checked_wal_sum(&records.wals, |record| record.mutation_count)
    else {
        audit_sum_overflow(data_source, "WAL mutation count");
        return;
    };
    let Some(latest_point_mutation_count) =
        checked_latest_state_sum(&records.latest_state, |record| record.point_mutation_count)
    else {
        audit_sum_overflow(data_source, "latest-state point mutation count");
        return;
    };
    let Some(latest_range_mutation_count) =
        checked_latest_state_sum(&records.latest_state, |record| record.range_mutation_count)
    else {
        audit_sum_overflow(data_source, "latest-state range mutation count");
        return;
    };
    let Some(latest_value_count) =
        checked_latest_state_sum(&records.latest_state, |record| record.latest_value_count)
    else {
        audit_sum_overflow(data_source, "latest-state value count");
        return;
    };
    let Some(latest_deleted_key_count) =
        checked_latest_state_sum(&records.latest_state, |record| record.deleted_key_count)
    else {
        audit_sum_overflow(data_source, "latest-state deleted key count");
        return;
    };
    let details = audit_details(
        records,
        CephMetadataAuditTotals {
            data_block_count,
            entry_count,
            data_bytes,
            wal_record_count,
            wal_mutation_count,
            latest_point_mutation_count,
            latest_range_mutation_count,
            latest_value_count,
            latest_deleted_key_count,
        },
    );
    if let Err(error) = AuditRepo::new(ctx.conn).log(
        Some(&ctx.case_id.0),
        "system",
        &AuditAction::DataSourceImport,
        Some(&data_source.id.0),
        &details,
    ) {
        tracing::warn!(
            data_source_id = %data_source.id.0,
            error = %error,
            "Failed to record BlueFS inventory audit entry"
        );
    }
}

fn audit_details(records: &CephMetadataAggregate, totals: CephMetadataAuditTotals) -> String {
    serde_json::json!({
        "inventoryId": records.bluefs.superblock.inventory_id,
        "osdUuid": records.bluefs.superblock.osd_uuid,
        "bluefsUuid": records.bluefs.superblock.bluefs_uuid,
        "sequence": records.bluefs.superblock.sequence,
        "extentCount": records.bluefs.log_extents.len(),
        "transactionCount": records.bluefs.replay.replay.transaction_count,
        "fileCount": records.bluefs.replay.files.len(),
        "directoryCount": records.bluefs.replay.directories.len(),
        "finalSequence": records.bluefs.replay.replay.final_sequence,
        "rocksdbManifest": records.rocksdb.manifest.active_manifest_path,
        "rocksdbIdentityPresent": records.rocksdb.manifest.identity_uuid.is_some(),
        "rocksdbLogicalEditCount": records.rocksdb.manifest.logical_edit_count,
        "rocksdbColumnFamilyCount": records.rocksdb.column_families.len(),
        "rocksdbLiveSstCount": records.rocksdb.live_ssts.len(),
        "rocksdbValidatedSstCount": records.ssts.len(),
        "rocksdbSstDataBlockCount": totals.data_block_count,
        "rocksdbSstEntryCount": totals.entry_count,
        "rocksdbSstDataBytes": totals.data_bytes,
        "rocksdbLastSequence": records.rocksdb.manifest.last_sequence,
        "rocksdbRecoveryWalCount": records.wals.files.len(),
        "rocksdbRecoveryWalRecordCount": totals.wal_record_count,
        "rocksdbRecoveryWalMutationCount": totals.wal_mutation_count,
        "rocksdbLatestStateColumnFamilyCount": records.latest_state.len(),
        "rocksdbLatestStatePointMutationCount": totals.latest_point_mutation_count,
        "rocksdbLatestStateRangeMutationCount": totals.latest_range_mutation_count,
        "rocksdbLatestStateValueCount": totals.latest_value_count,
        "rocksdbLatestStateDeletedKeyCount": totals.latest_deleted_key_count,
        "bluestoreSemanticSchemaVersion": records.semantic.scan.schema_version,
        "bluestoreSemanticDecodeProfile": records.semantic.scan.decode_profile,
        "bluestoreSemanticLatestStateSha256": records.semantic.scan.latest_state_sha256,
        "bluestoreSemanticSha256": records.semantic.scan.semantic_sha256,
        "bluestoreSemanticCollectionCount": records.semantic.scan.collection_count,
        "bluestoreSemanticObjectCount": records.semantic.scan.object_count,
        "bluestoreSemanticBlobCount": records.semantic.scan.blob_count,
        "bluestoreSemanticLogicalExtentCount": records.semantic.scan.logical_extent_count,
        "bluestoreSemanticPhysicalExtentCount": records.semantic.scan.physical_extent_count,
        "bluestoreSemanticSharedBlobCount": records.semantic.scan.shared_blob_count,
        "bluestoreOmapScopeCount": records.omap.scopes.len(),
        "rbdDirectoryMappingCount": records.omap.directory_mappings.len(),
        "rbdHeaderCount": records.omap.rbd_headers.len(),
        "layout": "singleSharedDevice",
    })
    .to_string()
}

fn checked_latest_state_sum(
    records: &[CephRocksdbLatestStateRecord],
    value: impl Fn(&CephRocksdbLatestStateRecord) -> u64,
) -> Option<u64> {
    records
        .iter()
        .try_fold(0u64, |total, record| total.checked_add(value(record)))
}

fn checked_wal_sum(
    records: &CephRocksdbWalAggregate,
    value: impl Fn(
        &persistence_sqlite::repositories::ceph_rocksdb_wal_repo::CephRocksdbWalFileRecord,
    ) -> u64,
) -> Option<u64> {
    records
        .files
        .iter()
        .try_fold(0u64, |total, record| total.checked_add(value(record)))
}

fn checked_sst_sum(
    records: &[CephRocksdbSstRecord],
    value: impl Fn(&CephRocksdbSstRecord) -> u64,
) -> Option<u64> {
    records
        .iter()
        .try_fold(0u64, |total, record| total.checked_add(value(record)))
}

fn audit_sum_overflow(data_source: &domain::DataSource, field: &str) {
    tracing::warn!(
        data_source_id = %data_source.id.0,
        field,
        "Skipped BlueFS inventory audit entry because a metadata aggregate overflowed"
    );
}
