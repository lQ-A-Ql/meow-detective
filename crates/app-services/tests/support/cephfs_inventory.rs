use persistence_sqlite::{
    open_in_memory,
    repositories::{
        ceph_bluestore_semantic_repo::latest_state_set_sha256,
        ceph_rocksdb_latest_state_repo::{
            CephRocksdbLatestStateRecord, CephRocksdbLatestStateRepo,
        },
    },
    runner,
};
use rusqlite::{params, Connection};

pub const INVENTORY: &str = "inventory-a";
pub const SOURCE: &str = "source-a";

pub fn source_with_metadata_objects() -> Connection {
    let conn = open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    seed_control_plane(&conn);
    seed_semantic_scan(&conn);
    insert_object(&conn, "1".repeat(64), b"1.00000000", b"", "parsed");
    insert_object(&conn, "2".repeat(64), &[0xff, 0x00], b"binary-ns", "parsed");
    conn
}

fn seed_control_plane(conn: &Connection) {
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, imported_at
         ) VALUES (?1, 'case-1', ?1, 'e01', ?1, '2026-07-19T00:00:00Z')",
        [SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_osd_inventory (
            id, data_source_id, osd_uuid, device_role, device_size,
            birth_time_seconds, birth_time_nanoseconds, description, is_multi,
            valid_label_count, label_health, osd_key_present, sanitized_metadata_json
         ) VALUES (?1, ?2, ?1, 'block', 1048576, 1, 0, 'BlueStore OSD', 1,
                   1, 'singleReplica', 1, '{}')",
        [INVENTORY, SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_bluefs_superblocks (
            inventory_id, data_source_id, bluefs_uuid, osd_uuid, sequence,
            block_size, crc32c, struct_version, struct_compat_version, log_inode,
            log_size, log_mtime_seconds, log_mtime_nanoseconds, log_encoding,
            log_content_size, shared_bdev, dedicated_db, dedicated_wal
         ) VALUES (?1, ?2, ?1, ?1, 10, 4096, 1, 2, 1, 1, 4096, 1, 0, 0,
                   4096, 1, 0, 0)",
        [INVENTORY, SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_bluefs_replays (
            inventory_id, transaction_count, first_sequence, final_sequence,
            logical_bytes, stop_reason
         ) VALUES (?1, 1, 1, 10, 4096, 'invalidTail')",
        [INVENTORY],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_rocksdb_manifests (
            inventory_id, data_source_id, active_manifest_path, manifest_file_number,
            manifest_file_size, logical_edit_count, comparator_name, last_sequence,
            next_file_number, log_number, prev_log_number, max_column_family_id
         ) VALUES (?1, ?2, 'db/MANIFEST-000143', 143, 4096, 10,
                   'leveldb.BytewiseComparator', 100, 150, 142, 0, 0)",
        [INVENTORY, SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ceph_rocksdb_column_families (
            inventory_id, column_family_id, name, comparator_name, dropped, log_number
         ) VALUES (?1, 0, 'default', 'leveldb.BytewiseComparator', 0, 142)",
        [INVENTORY],
    )
    .unwrap();
    CephRocksdbLatestStateRepo::new(conn)
        .replace_for_inventory(INVENTORY, &latest_state())
        .unwrap();
}

fn seed_semantic_scan(conn: &Connection) {
    conn.execute(
        "INSERT INTO ceph_bluestore_semantic_scans (
            inventory_id, schema_version, decode_profile, sharding_sha256,
            latest_state_sha256, semantic_sha256,
            s_latest_count, s_decoded_count, s_deferred_count,
            c_latest_count, c_decoded_count, c_deferred_count,
            o_latest_count, o_decoded_count, o_deferred_count,
            x_latest_count, x_decoded_count, x_deferred_count,
            collection_count, object_count, blob_count, onode_shard_count,
            logical_extent_count, physical_extent_count, checksum_chunk_count,
            shared_blob_count, shared_ref_extent_count, profile_complete
         ) VALUES (?1, 1, 'scox-v1', ?2, ?3, ?4,
                   0, 0, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0,
                   0, 2, 0, 0, 0, 0, 0, 0, 0, 1)",
        params![
            INVENTORY,
            "a".repeat(64),
            latest_state_set_sha256(&latest_state()),
            "c".repeat(64),
        ],
    )
    .unwrap();
}

fn latest_state() -> Vec<CephRocksdbLatestStateRecord> {
    vec![CephRocksdbLatestStateRecord {
        inventory_id: INVENTORY.to_string(),
        column_family_id: 0,
        column_family_name: "default".to_string(),
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
    }]
}

fn insert_object(
    conn: &Connection,
    identity: String,
    name: &[u8],
    namespace: &[u8],
    decode_status: &str,
) {
    conn.execute(
        "INSERT INTO ceph_bluestore_objects (
            inventory_id, object_identity_sha256, decoded_shard, decoded_pool,
            decoded_hash, decoded_bitwise_hash, object_namespace, object_key,
            object_name, snap_hex, generation_hex, onode_denc_version, nid, size,
            flags_raw, flag_omap, flag_pgmeta_omap, flag_per_pool_omap,
            flag_per_pg_omap, flags_unknown_bits, attribute_count,
            attribute_value_bytes, attributes_sha256, expected_object_size,
            expected_write_size, allocation_hint_flags, zone_ref_count,
            extent_storage, spanning_blob_version, declared_spanning_blob_count,
            decode_status, deferred_reason, onode_shard_count, blob_count,
            logical_extent_count, physical_extent_count
         ) VALUES (
            ?1, ?2, -1, 7, 1, 2147483648, ?3, NULL, ?4,
            'fffffffffffffffe', '0000000000000000', 1, 1, 32,
            0, 0, 0, 0, 0, 0, 0, 0, ?5, 32, 32, 0, 0,
            'inline', 0, 0, ?6, NULL, 0, 0, 0, 0)",
        params![
            INVENTORY,
            identity,
            namespace,
            name,
            "e".repeat(64),
            decode_status,
        ],
    )
    .unwrap();
}
