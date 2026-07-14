use persistence_sqlite::repositories::{
    ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord,
    ceph_rocksdb_repo::CephRocksdbAggregate,
};

pub fn empty_latest_state(rocksdb: &CephRocksdbAggregate) -> Vec<CephRocksdbLatestStateRecord> {
    rocksdb
        .column_families
        .iter()
        .filter(|column_family| !column_family.dropped)
        .map(|column_family| CephRocksdbLatestStateRecord {
            inventory_id: rocksdb.manifest.inventory_id.clone(),
            column_family_id: column_family.column_family_id,
            column_family_name: column_family.name.clone(),
            schema_version: 1,
            sharding_sha256: "a".repeat(64),
            point_mutation_count: 0,
            sst_point_mutation_count: 0,
            wal_point_mutation_count: 0,
            range_mutation_count: 0,
            sst_range_mutation_count: 0,
            wal_range_mutation_count: 0,
            latest_value_count: 0,
            deleted_key_count: 0,
            delete_decision_count: 0,
            single_delete_decision_count: 0,
            range_delete_decision_count: 0,
            merge_resolved_count: 0,
            merge_operand_count: 0,
            range_hidden_version_count: 0,
            smallest_sequence: None,
            largest_sequence: None,
            point_sha256: "b".repeat(64),
            range_sha256: "c".repeat(64),
            latest_state_sha256: "d".repeat(64),
            scan_complete: true,
        })
        .collect()
}
