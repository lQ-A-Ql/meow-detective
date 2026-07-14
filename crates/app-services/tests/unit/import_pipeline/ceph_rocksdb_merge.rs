use super::full_merge;

fn sharding() -> crate::import_pipeline::ceph_rocksdb_sharding::RocksdbShardingDefinition {
    crate::import_pipeline::ceph_rocksdb_sharding::parse_rocksdb_sharding_definition(
        "m(3) p(3) O(3) L P",
    )
    .expect("parse sharding definition")
}

#[test]
fn applies_ceph_int64_array_in_chronological_order() {
    let definition = sharding();
    let one = 1u64.to_le_bytes();
    let two = 2u64.to_le_bytes();
    let three = 3u64.to_le_bytes();
    let merged = full_merge(
        &definition,
        "default",
        b"T\0stat",
        Some(&one),
        &[&two, &three],
    )
    .expect("merge int64 array");
    assert_eq!(merged, 6u64.to_le_bytes());
}

#[test]
fn applies_ceph_bitmap_xor_and_rejects_unregistered_prefixes() {
    let definition = sharding();
    let merged = full_merge(
        &definition,
        "default",
        b"b\0bitmap",
        Some(&[0b1010]),
        &[&[0b1100], &[0b0011]],
    )
    .expect("merge bitmap");
    assert_eq!(merged, vec![0b0101]);
    assert!(full_merge(&definition, "default", b"O\0object", None, &[b"value"]).is_err());
}
