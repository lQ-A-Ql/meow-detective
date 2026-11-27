use domain::structure::{
    ByteRange, ByteRangeError, ByteRangeSet, ByteRangeSetError, ReadDemand, ReadIntent,
    ReadPriority, ReadVerification, SourceRevisionRef,
};

#[test]
fn range_rejects_address_overflow_and_target_overrun() {
    assert_eq!(ByteRange::new(u64::MAX, 1), Err(ByteRangeError::Overflow));
    let range = ByteRange::new(4093, 8177).unwrap();
    assert_eq!(range.start(), 4093);
    assert_eq!(range.length(), 8177);
    assert_eq!(range.end(), 12270);
    assert_eq!(
        range.validate(12269),
        Err(ByteRangeError::OutOfBounds {
            logical_size: 12269
        })
    );
    range.validate(12270).unwrap();
}

#[test]
fn empty_range_is_valid_only_inside_target_address_space() {
    ByteRange::new(0, 0).unwrap().validate(0).unwrap();
    ByteRange::new(32, 0).unwrap().validate(32).unwrap();
    assert!(ByteRange::new(33, 0).unwrap().validate(32).is_err());
    ByteRange::new(u64::MAX, 0)
        .unwrap()
        .validate(u64::MAX)
        .unwrap();
}

#[test]
fn range_set_merges_overlap_and_rejects_gaps_outside_the_target() {
    let ranges = ByteRangeSet::new(
        4096,
        [
            ByteRange::new(1024, 512).unwrap(),
            ByteRange::new(0, 1024).unwrap(),
            ByteRange::new(1500, 100).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(ranges.ranges(), &[ByteRange::new(0, 1600).unwrap()]);
    assert_eq!(ranges.covered_bytes(), 1600);
    assert!(matches!(
        ByteRangeSet::new(4096, [ByteRange::new(4000, 100).unwrap()]),
        Err(ByteRangeSetError::Range(_))
    ));
    assert!(matches!(
        ByteRangeSet::new(4096, Vec::<ByteRange>::new()),
        Err(ByteRangeSetError::Empty)
    ));
}

#[test]
fn read_demand_binds_revision_priority_consumers_and_attempt() {
    let demand = ReadDemand::new(
        SourceRevisionRef {
            case_id: "case".into(),
            source_id: "source".into(),
            revision: 3,
        },
        "target".into(),
        ReadIntent::ContentRead,
        ByteRangeSet::new(4096, [ByteRange::new(0, 4096).unwrap()]).unwrap(),
        ReadPriority::Interactive,
        Some(1_700_000_000_000),
        true,
        vec!["preview".into(), "verification".into()],
        ReadVerification::ContentDigest,
    )
    .unwrap()
    .with_attempt(domain::structure::StructureAttemptRef {
        attempt_id: "attempt".into(),
        owner: "worker".into(),
    })
    .unwrap();
    assert!(demand.id.starts_with("read-demand-v1-"));
    assert_eq!(demand.source_revision.revision, 3);
    assert_eq!(demand.priority, ReadPriority::Interactive);
    assert_eq!(demand.consumers.len(), 2);
    assert_eq!(demand.attempt.unwrap().attempt_id, "attempt");
}
