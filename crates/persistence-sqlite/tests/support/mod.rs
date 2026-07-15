use persistence_sqlite::repositories::{
    ceph_bluestore_semantic_repo::{
        latest_state_set_sha256, semantic_aggregate_sha256, CephBluestoreSemanticAggregate,
        CephBluestoreSemanticScanRecord, CephBluestoreSuperRecord,
        BLUESTORE_SEMANTIC_DECODE_PROFILE, BLUESTORE_SEMANTIC_SCHEMA_VERSION,
    },
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

pub fn empty_semantic(
    rocksdb: &CephRocksdbAggregate,
    latest_state: &[CephRocksdbLatestStateRecord],
) -> CephBluestoreSemanticAggregate {
    let inventory_id = rocksdb.manifest.inventory_id.clone();
    let mut aggregate = CephBluestoreSemanticAggregate {
        scan: CephBluestoreSemanticScanRecord {
            inventory_id: inventory_id.clone(),
            schema_version: BLUESTORE_SEMANTIC_SCHEMA_VERSION,
            decode_profile: BLUESTORE_SEMANTIC_DECODE_PROFILE.to_string(),
            sharding_sha256: latest_state
                .first()
                .map(|record| record.sharding_sha256.clone())
                .unwrap_or_else(|| "a".repeat(64)),
            latest_state_sha256: latest_state_set_sha256(latest_state),
            semantic_sha256: String::new(),
            s_latest_count: 0,
            s_decoded_count: 0,
            s_deferred_count: 0,
            c_latest_count: 0,
            c_decoded_count: 0,
            c_deferred_count: 0,
            o_latest_count: 0,
            o_decoded_count: 0,
            o_deferred_count: 0,
            x_latest_count: 0,
            x_decoded_count: 0,
            x_deferred_count: 0,
            collection_count: 0,
            object_count: 0,
            blob_count: 0,
            onode_shard_count: 0,
            logical_extent_count: 0,
            physical_extent_count: 0,
            checksum_chunk_count: 0,
            shared_blob_count: 0,
            shared_ref_extent_count: 0,
            profile_complete: true,
        },
        super_record: CephBluestoreSuperRecord {
            inventory_id,
            nid_max: None,
            blobid_max: None,
            min_alloc_size: None,
            ondisk_format: None,
            min_compat_ondisk_format: None,
            per_pool_omap: None,
            freelist_type: None,
            observed_count: 0,
            deferred_count: 0,
        },
        collections: Vec::new(),
        objects: Vec::new(),
        onode_shards: Vec::new(),
        blobs: Vec::new(),
        logical_extents: Vec::new(),
        physical_extents: Vec::new(),
        checksum_chunks: Vec::new(),
        shared_blobs: Vec::new(),
        shared_blob_refs: Vec::new(),
    };
    aggregate.scan.semantic_sha256 = semantic_aggregate_sha256(&aggregate);
    aggregate
}
