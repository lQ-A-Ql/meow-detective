use persistence_sqlite::repositories::ceph_rocksdb_repo::{
    CephRocksdbAggregate, CephRocksdbColumnFamilyRecord, CephRocksdbManifestRecord,
};

use super::{parse_rocksdb_sharding_definition, validate_active_column_families};

const SAMPLE: &str = "m(3) p(3,0-12) O(3,0-13)=block_cache={type=binned_lru} L=min_write_buffer_number_to_merge=32 P=min_write_buffer_number_to_merge=32";

fn rocksdb(names: &[&str]) -> CephRocksdbAggregate {
    CephRocksdbAggregate {
        manifest: CephRocksdbManifestRecord {
            inventory_id: "inventory-1".to_string(),
            data_source_id: "source-1".to_string(),
            active_manifest_path: "db/MANIFEST-000143".to_string(),
            identity_uuid: None,
            manifest_file_number: 143,
            manifest_file_size: 1,
            logical_edit_count: 1,
            comparator_name: "leveldb.BytewiseComparator".to_string(),
            last_sequence: 1,
            next_file_number: 148,
            log_number: 1,
            prev_log_number: 0,
            max_column_family_id: names.len() as u32 - 1,
            min_log_number_to_keep: None,
        },
        column_families: names
            .iter()
            .enumerate()
            .map(|(id, name)| CephRocksdbColumnFamilyRecord {
                inventory_id: "inventory-1".to_string(),
                column_family_id: id as u32,
                name: (*name).to_string(),
                comparator_name: "leveldb.BytewiseComparator".to_string(),
                dropped: false,
            })
            .collect(),
        live_ssts: Vec::new(),
    }
}

#[test]
fn parses_real_reef_sharding_definition_and_expands_routes() {
    let definition = parse_rocksdb_sharding_definition(SAMPLE).expect("parse sharding");

    let default = definition.route("default").expect("default route");
    assert!(!default.strips_logical_prefix);
    assert!(default.logical_prefix.is_none());

    let object = definition.route("O-2").expect("object route");
    assert_eq!(object.logical_prefix.as_deref(), Some("O"));
    assert_eq!(object.shard_index, Some(2));
    assert_eq!((object.hash_low, object.hash_high), (0, 13));
    assert!(object.strips_logical_prefix);
    assert!(format!("{:?}", definition.census_context("O-2").unwrap()).contains("bluestore.object"));

    let deferred = definition.route("L").expect("deferred route");
    assert_eq!(deferred.logical_prefix.as_deref(), Some("L"));
    assert!(deferred.shard_index.is_none());
}

#[test]
fn validates_sharding_against_the_manifest_active_cf_set() {
    let definition = parse_rocksdb_sharding_definition(SAMPLE).expect("parse sharding");
    let names = [
        "default", "m-0", "m-1", "m-2", "p-0", "p-1", "p-2", "O-0", "O-1", "O-2", "L", "P",
    ];
    assert!(validate_active_column_families(&definition, &rocksdb(&names)).is_ok());

    let mut missing = names.to_vec();
    missing.pop();
    assert!(validate_active_column_families(&definition, &rocksdb(&missing)).is_err());
}

#[test]
fn rejects_noncanonical_or_unsafe_definitions() {
    for invalid in [
        " m(3)",
        "m(0)",
        "m(65)",
        "m(3,13-0)",
        "m(3,0)",
        "m(3))",
        "m(3)  p(3)",
        "m(3)=block_cache={type=lru",
        "m(3)\np(3)",
        "m\u{0}(3)",
    ] {
        assert!(
            parse_rocksdb_sharding_definition(invalid).is_err(),
            "accepted invalid sharding definition: {invalid:?}"
        );
    }
}

#[test]
fn accepts_an_unsharded_default_only_database() {
    let definition =
        parse_rocksdb_sharding_definition("").expect("parse default-only sharding definition");

    assert!(definition.route("default").is_some());
    assert!(validate_active_column_families(&definition, &rocksdb(&["default"])).is_ok());
}
