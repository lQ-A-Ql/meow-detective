use domain::DataSourceId;
use persistence_sqlite::repositories::ceph_rocksdb_repo::{
    CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbManifestRecord,
};
use tempfile::TempDir;

use super::recover_latest_state;
use crate::import_pipeline::{
    ceph_rocksdb_sharding::parse_rocksdb_sharding_definition,
    ceph_rocksdb_spool::{
        RocksdbRecoverySpool, SpoolPointInput, SpoolProvenance, SpoolRangeInput, SpoolSourceKind,
    },
};

fn provenance(source_kind: SpoolSourceKind, file_number: u64) -> SpoolProvenance {
    SpoolProvenance {
        source_kind,
        file_number,
        level: (source_kind == SpoolSourceKind::Sst).then_some(1),
        physical_offset: 4096,
        primary_ordinal: file_number,
        secondary_ordinal: 0,
    }
}

fn rocksdb() -> CephRocksdbAggregate {
    CephRocksdbAggregate {
        manifest: CephRocksdbManifestRecord {
            inventory_id: "inventory-1".to_string(),
            data_source_id: "source-1".to_string(),
            active_manifest_path: "db/MANIFEST-000010".to_string(),
            identity_uuid: None,
            manifest_file_number: 10,
            manifest_file_size: 4096,
            logical_edit_count: 1,
            comparator_name: "leveldb.BytewiseComparator".to_string(),
            last_sequence: 30,
            next_file_number: 20,
            log_number: 9,
            prev_log_number: 0,
            max_column_family_id: 1,
            min_log_number_to_keep: Some(9),
        },
        column_families: vec![column_family(0, "default"), column_family(1, "m")],
        live_ssts: Vec::new(),
    }
}

fn column_family(column_family_id: u32, name: &str) -> CephRocksdbColumnFamilyRecord {
    CephRocksdbColumnFamilyRecord {
        inventory_id: "inventory-1".to_string(),
        column_family_id,
        name: name.to_string(),
        comparator_name: "leveldb.BytewiseComparator".to_string(),
        log_number: Some(9),
        dropped: false,
    }
}

fn insert_point(
    spool: &mut RocksdbRecoverySpool,
    column_family_id: u32,
    key: &[u8],
    sequence: u64,
    value_type: u8,
    value: &[u8],
    provenance: SpoolProvenance,
) {
    spool
        .insert_point(SpoolPointInput {
            column_family_id,
            user_key: key,
            sequence,
            value_type,
            value,
            provenance,
        })
        .expect("insert point");
}

#[test]
fn produces_digest_only_summary_for_every_active_column_family() {
    let case = TempDir::new().expect("case root");
    let mut spool =
        RocksdbRecoverySpool::create(case.path(), &DataSourceId("source-1".to_string()))
            .expect("create spool");
    insert_point(
        &mut spool,
        0,
        b"a",
        12,
        1,
        b"latest",
        provenance(SpoolSourceKind::Wal, 12),
    );
    insert_point(
        &mut spool,
        0,
        b"T\0stat",
        30,
        2,
        &3u64.to_le_bytes(),
        provenance(SpoolSourceKind::Sst, 13),
    );
    insert_point(
        &mut spool,
        0,
        b"T\0stat",
        20,
        2,
        &2u64.to_le_bytes(),
        provenance(SpoolSourceKind::Sst, 14),
    );
    insert_point(
        &mut spool,
        0,
        b"T\0stat",
        10,
        1,
        &1u64.to_le_bytes(),
        provenance(SpoolSourceKind::Sst, 15),
    );
    insert_point(
        &mut spool,
        0,
        b"deleted",
        20,
        0,
        b"",
        provenance(SpoolSourceKind::Sst, 16),
    );
    insert_point(
        &mut spool,
        0,
        b"deleted",
        10,
        1,
        b"old",
        provenance(SpoolSourceKind::Sst, 17),
    );
    insert_point(
        &mut spool,
        0,
        b"ranged",
        5,
        1,
        b"hidden",
        provenance(SpoolSourceKind::Sst, 18),
    );
    insert_point(
        &mut spool,
        1,
        b"object",
        9,
        1,
        b"metadata",
        provenance(SpoolSourceKind::Sst, 19),
    );
    spool
        .insert_range(SpoolRangeInput {
            column_family_id: 0,
            start_key: b"range",
            end_key: b"rangez",
            sequence: 8,
            provenance: provenance(SpoolSourceKind::Wal, 20),
        })
        .expect("insert range");
    spool.seal().expect("seal spool");

    let sharding = parse_rocksdb_sharding_definition("m").expect("parse sharding");
    let records =
        recover_latest_state(&rocksdb(), &sharding, &spool).expect("recover latest state");
    assert_eq!(records.len(), 2);
    let default = &records[0];
    assert_eq!(default.column_family_name, "default");
    assert_eq!(default.point_mutation_count, 7);
    assert_eq!(default.sst_point_mutation_count, 6);
    assert_eq!(default.wal_point_mutation_count, 1);
    assert_eq!(default.range_mutation_count, 1);
    assert_eq!(default.wal_range_mutation_count, 1);
    assert_eq!(default.latest_value_count, 2);
    assert_eq!(default.deleted_key_count, 2);
    assert_eq!(default.delete_decision_count, 1);
    assert_eq!(default.range_delete_decision_count, 1);
    assert_eq!(default.merge_resolved_count, 1);
    assert_eq!(default.merge_operand_count, 2);
    assert_eq!(default.range_hidden_version_count, 1);
    assert_eq!(default.smallest_sequence, Some(5));
    assert_eq!(default.largest_sequence, Some(30));
    for digest in [
        &default.sharding_sha256,
        &default.point_sha256,
        &default.range_sha256,
        &default.latest_state_sha256,
    ] {
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    let sharded = &records[1];
    assert_eq!(sharded.column_family_name, "m");
    assert_eq!(sharded.point_mutation_count, 1);
    assert_eq!(sharded.latest_value_count, 1);
    assert_eq!(sharded.deleted_key_count, 0);
    assert_eq!(sharded.smallest_sequence, Some(9));
    assert_eq!(sharded.largest_sequence, Some(9));
    assert_eq!(sharded.sharding_sha256, default.sharding_sha256);
}

#[test]
fn rejects_mutations_for_unknown_column_families() {
    let case = TempDir::new().expect("case root");
    let mut spool =
        RocksdbRecoverySpool::create(case.path(), &DataSourceId("source-1".to_string()))
            .expect("create spool");
    insert_point(
        &mut spool,
        9,
        b"unknown",
        1,
        1,
        b"value",
        provenance(SpoolSourceKind::Sst, 1),
    );
    spool.seal().expect("seal spool");
    let sharding = parse_rocksdb_sharding_definition("m").expect("parse sharding");

    assert!(recover_latest_state(&rocksdb(), &sharding, &spool).is_err());
}

#[test]
fn point_only_recovery_reads_column_families_through_separate_connections() {
    let case = TempDir::new().expect("case root");
    let mut spool =
        RocksdbRecoverySpool::create(case.path(), &DataSourceId("source-1".to_string()))
            .expect("create spool");
    insert_point(
        &mut spool,
        0,
        b"key",
        2,
        1,
        b"latest",
        provenance(SpoolSourceKind::Sst, 1),
    );
    insert_point(
        &mut spool,
        0,
        b"key",
        1,
        0,
        b"",
        provenance(SpoolSourceKind::Sst, 2),
    );
    insert_point(
        &mut spool,
        1,
        b"other",
        3,
        7,
        b"",
        provenance(SpoolSourceKind::Wal, 3),
    );
    spool.seal().expect("seal spool");
    let sharding = parse_rocksdb_sharding_definition("m").expect("parse sharding");

    let records =
        recover_latest_state(&rocksdb(), &sharding, &spool).expect("recover point-only state");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].point_mutation_count, 2);
    assert_eq!(records[0].latest_value_count, 1);
    assert_eq!(records[1].point_mutation_count, 1);
    assert_eq!(records[1].single_delete_decision_count, 1);
}
