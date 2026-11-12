use super::RangeCoverage;
use crate::import_pipeline::ceph_rocksdb_spool::{SpoolProvenance, SpoolRange, SpoolSourceKind};

fn range(start: &[u8], end: &[u8], sequence: u64) -> SpoolRange {
    SpoolRange {
        column_family_id: 0,
        start_key: start.to_vec(),
        end_key: end.to_vec(),
        sequence,
        provenance: SpoolProvenance {
            source_kind: SpoolSourceKind::Sst,
            file_number: sequence + 1,
            level: Some(1),
            physical_offset: 0,
            primary_ordinal: 0,
            secondary_ordinal: 0,
        },
    }
}

#[test]
fn selects_highest_sequence_across_overlapping_and_nested_ranges() {
    let mut coverage = RangeCoverage::new(vec![
        range(b"a", b"z", 5),
        range(b"b", b"d", 9),
        range(b"c", b"e", 20),
    ]);
    assert_eq!(coverage.covering_sequence(b"a"), Some(5));
    assert_eq!(coverage.covering_sequence(b"b"), Some(9));
    assert_eq!(coverage.covering_sequence(b"c"), Some(20));
    assert_eq!(coverage.covering_sequence(b"d"), Some(20));
    assert_eq!(coverage.covering_sequence(b"e"), Some(5));
    assert_eq!(coverage.covering_sequence(b"z"), None);
}
