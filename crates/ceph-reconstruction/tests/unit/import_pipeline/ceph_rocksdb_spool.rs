use domain::DataSourceId;
use tempfile::TempDir;

use super::{
    RocksdbRecoverySpool, SpoolPointInput, SpoolProvenance, SpoolRangeInput, SpoolSourceKind,
    MAX_RESIDENT_RANGE_BYTES,
};

fn provenance(source_kind: SpoolSourceKind, file_number: u64) -> SpoolProvenance {
    SpoolProvenance {
        source_kind,
        file_number,
        level: (source_kind == SpoolSourceKind::Sst).then_some(2),
        physical_offset: 4096,
        primary_ordinal: 3,
        secondary_ordinal: 4,
    }
}

#[test]
fn stores_sorted_point_groups_and_ranges_inside_case_staging() {
    let case = TempDir::new().expect("case root");
    let data_source_id = DataSourceId("source-a".to_string());
    let mut spool =
        RocksdbRecoverySpool::create(case.path(), &data_source_id).expect("create spool");
    let path = spool.path().to_path_buf();
    assert!(path.starts_with(case.path().join("staging/source-a")));

    spool
        .insert_point(SpoolPointInput {
            column_family_id: 1,
            user_key: b"key-b",
            sequence: 5,
            value_type: 1,
            value: b"five",
            provenance: provenance(SpoolSourceKind::Sst, 10),
        })
        .expect("insert point");
    spool
        .insert_point(SpoolPointInput {
            column_family_id: 1,
            user_key: b"key-a",
            sequence: 7,
            value_type: 2,
            value: b"seven",
            provenance: provenance(SpoolSourceKind::Wal, 11),
        })
        .expect("insert newer point");
    spool
        .insert_point(SpoolPointInput {
            column_family_id: 1,
            user_key: b"key-a",
            sequence: 6,
            value_type: 1,
            value: b"six",
            provenance: provenance(SpoolSourceKind::Sst, 12),
        })
        .expect("insert older point");
    spool
        .insert_range(SpoolRangeInput {
            column_family_id: 1,
            start_key: b"key-a",
            end_key: b"key-z",
            sequence: 4,
            provenance: provenance(SpoolSourceKind::Wal, 13),
        })
        .expect("insert range");
    spool.seal().expect("seal spool");

    let mut groups = Vec::new();
    spool
        .visit_point_groups(|group| {
            groups.push(
                group
                    .iter()
                    .map(|point| (point.user_key.clone(), point.sequence))
                    .collect::<Vec<_>>(),
            );
            Ok(())
        })
        .expect("visit groups");
    assert_eq!(
        groups,
        vec![
            vec![(b"key-a".to_vec(), 7), (b"key-a".to_vec(), 6)],
            vec![(b"key-b".to_vec(), 5)],
        ]
    );
    let ranges = spool.load_ranges().expect("load ranges");
    assert_eq!(ranges.len(), 1);
    assert_eq!(spool.point_count(), 3);
    assert_eq!(spool.range_count(), 1);
    assert_eq!(spool.merge_count(), 1);
    assert_eq!(spool.raw_bytes(), 37);

    drop(spool);
    assert!(!path.exists(), "temporary raw KV spool must be deleted");
}

#[test]
fn rejects_duplicate_sequences_and_discards_unsealed_output() {
    let case = TempDir::new().expect("case root");
    let data_source_id = DataSourceId("source-b".to_string());
    let mut spool =
        RocksdbRecoverySpool::create(case.path(), &data_source_id).expect("create spool");
    let path = spool.path().to_path_buf();
    for value_type in [1, 2] {
        let result = spool.insert_point(SpoolPointInput {
            column_family_id: 0,
            user_key: b"same",
            sequence: 9,
            value_type,
            value: b"value",
            provenance: provenance(SpoolSourceKind::Sst, u64::from(value_type) + 1),
        });
        if value_type == 1 {
            result.expect("first point");
        } else {
            assert!(result.is_err(), "same sequence must be ambiguous");
        }
    }
    drop(spool);
    assert!(!path.exists(), "failed spool must be deleted");
}

#[test]
fn preserves_empty_range_tombstones_as_legal_no_ops() {
    let case = TempDir::new().expect("case root");
    let data_source_id = DataSourceId("source-empty-range".to_string());
    let mut spool =
        RocksdbRecoverySpool::create(case.path(), &data_source_id).expect("create spool");
    spool
        .insert_range(SpoolRangeInput {
            column_family_id: 1,
            start_key: b"same",
            end_key: b"same",
            sequence: 12,
            provenance: provenance(SpoolSourceKind::Wal, 20),
        })
        .expect("insert empty range tombstone");
    spool.seal().expect("seal spool");

    let ranges = spool.load_ranges().expect("load ranges");
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start_key, ranges[0].end_key);
}

#[test]
fn rejects_ranges_that_exceed_the_resident_byte_budget() {
    let case = TempDir::new().expect("case root");
    let data_source_id = DataSourceId("source-range-budget".to_string());
    let mut spool =
        RocksdbRecoverySpool::create(case.path(), &data_source_id).expect("create spool");
    spool.range_bytes = MAX_RESIDENT_RANGE_BYTES;

    let error = spool
        .insert_range(SpoolRangeInput {
            column_family_id: 1,
            start_key: b"a",
            end_key: b"b",
            sequence: 12,
            provenance: provenance(SpoolSourceKind::Sst, 20),
        })
        .expect_err("resident range-byte budget must be enforced before insertion");

    assert!(error.to_string().contains("resident range-byte limit"));
    assert_eq!(spool.range_count(), 0);
    assert_eq!(spool.raw_bytes(), 0);
}
