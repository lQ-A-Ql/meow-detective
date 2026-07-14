use super::{sharding_sha256, RecoveryDigests};
use crate::import_pipeline::ceph_rocksdb_spool::{
    SpoolPoint, SpoolPointRef, SpoolProvenance, SpoolSourceKind,
};

fn point() -> SpoolPoint {
    SpoolPoint {
        column_family_id: 1,
        user_key: b"key".to_vec(),
        sequence: 7,
        value_type: 1,
        value: b"value".to_vec(),
        provenance: SpoolProvenance {
            source_kind: SpoolSourceKind::Sst,
            file_number: 10,
            level: Some(2),
            physical_offset: 4096,
            primary_ordinal: 3,
            secondary_ordinal: 4,
        },
    }
}

#[test]
fn canonical_digests_are_deterministic_and_domain_separated() {
    let point = point();
    let mut first = RecoveryDigests::new(1, "m-0");
    first.observe_point_ref(point_ref(&point));
    first.observe_live(b"key", 7, 1, b"value");
    let first = first.finish();

    let mut second = RecoveryDigests::new(1, "m-0");
    second.observe_point_ref(point_ref(&point));
    second.observe_live(b"key", 7, 1, b"value");
    let second = second.finish();
    assert_eq!(first.point_sha256, second.point_sha256);
    assert_eq!(first.latest_state_sha256, second.latest_state_sha256);
    assert_ne!(first.point_sha256, first.latest_state_sha256);
    assert_eq!(first.range_sha256.len(), 64);
    assert_eq!(sharding_sha256(b"m(3)"), sharding_sha256(b"m(3)"));
}

fn point_ref(point: &SpoolPoint) -> SpoolPointRef<'_> {
    SpoolPointRef {
        column_family_id: point.column_family_id,
        user_key: &point.user_key,
        sequence: point.sequence,
        value_type: point.value_type,
        value: &point.value,
        provenance: point.provenance,
    }
}
