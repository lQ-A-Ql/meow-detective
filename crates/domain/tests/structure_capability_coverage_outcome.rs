use std::collections::BTreeSet;
use std::fmt::Debug;

use domain::structure::{
    ActivityState, ByteCoverage, ByteRange, CancellationScope, CapabilityError,
    CapabilityIntegrity, CapabilityKind, CapabilityRef, ConflictReason, CoverageContext,
    CoverageError, EventClock, EventClockBasis, EventCoverage, EventOrdering, EvidenceRequirement,
    KeyBoundary, KeyCoverage, KeyOrdering, MetadataFailure, NamespaceCoverage, NamespacePage,
    NamespacePageEnd, NativeKeyRange, NativeSequenceRange, OutcomeError, ResourceKind,
    RuntimeComponent, RuntimeCoverage, TargetCoverage, TargetFailure, TargetResult, TargetStatus,
    VersionBoundary,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

fn roundtrip<T: Debug + PartialEq + Serialize + DeserializeOwned>(value: &T) {
    let encoded = serde_json::to_string(value).unwrap();
    assert_eq!(&serde_json::from_str::<T>(&encoded).unwrap(), value);
}

fn range(start: u64, length: u64) -> ByteRange {
    ByteRange::new(start, length).unwrap()
}

fn content_verified() -> CapabilityIntegrity {
    CapabilityIntegrity::ContentVerified {
        manifest_sha256: hex::encode(Sha256::digest(b"target-content-manifest-v1")),
        content_sha256: hex::encode(Sha256::digest([0x5a; 16])),
    }
}

fn context() -> CoverageContext {
    CoverageContext {
        view_id: "view-1".into(),
        native_namespace: "native-namespace".into(),
        version: VersionBoundary {
            adapter_id: "native-adapter".into(),
            native_kind: "commit-boundary".into(),
            native_value: "epoch=17;term=3;index=9007199254740993".into(),
            witness_sha256: "a".repeat(64),
        },
    }
}

fn bytes() -> ByteCoverage {
    ByteCoverage {
        target_size: 16,
        requested: vec![range(0, 16)],
        verified: vec![range(0, 4)],
        native_holes: vec![range(4, 4)],
        unknown: vec![range(8, 4)],
        unreadable: vec![range(12, 4)],
    }
}

fn namespace() -> NamespaceCoverage {
    NamespaceCoverage {
        context: context(),
        root_id: "root-7".into(),
        query_sha256: "b".repeat(64),
        ordering_id: "native-directory-order-v1".into(),
        pages: vec![
            NamespacePage {
                input_cursor: None,
                first_ordinal: 0,
                object_count: 512,
                page_sha256: "c".repeat(64),
                verified: true,
                end: NamespacePageEnd::More {
                    cursor: "opaque-cursor-512".into(),
                },
            },
            NamespacePage {
                input_cursor: Some("opaque-cursor-512".into()),
                first_ordinal: 512,
                object_count: 1,
                page_sha256: "d".repeat(64),
                verified: true,
                end: NamespacePageEnd::Eof {
                    witness_sha256: "e".repeat(64),
                },
            },
        ],
    }
}

fn before(key: &[u8]) -> KeyBoundary {
    KeyBoundary::Before {
        native_key: key.to_vec(),
        sort_key: key.to_vec(),
    }
}

fn after(key: &[u8]) -> KeyBoundary {
    KeyBoundary::After {
        native_key: key.to_vec(),
        sort_key: key.to_vec(),
    }
}

fn key_range(lower: KeyBoundary, upper: KeyBoundary) -> NativeKeyRange {
    NativeKeyRange { lower, upper }
}

fn keys() -> KeyCoverage {
    KeyCoverage {
        context: context(),
        ordering: KeyOrdering {
            comparator_id: "bytewise".into(),
            sort_key_encoding_id: "identity-bytes-v1".into(),
            witness_sha256: "f".repeat(64),
        },
        requested: vec![key_range(
            KeyBoundary::UnboundedStart,
            KeyBoundary::UnboundedEnd,
        )],
        verified: vec![key_range(KeyBoundary::UnboundedStart, before(b"deleted"))],
        tombstones: vec![key_range(before(b"deleted"), after(b"deleted"))],
        unknown: vec![key_range(after(b"deleted"), before(b"unreadable"))],
        unreadable: vec![key_range(before(b"unreadable"), KeyBoundary::UnboundedEnd)],
    }
}

fn sequence(first: u64, last_inclusive: u64) -> NativeSequenceRange {
    NativeSequenceRange {
        first,
        last_inclusive,
    }
}

fn events() -> EventCoverage {
    EventCoverage {
        context: context(),
        sequence_domain: "wal-term-3".into(),
        native_ordering_id: "native-sequence-v1".into(),
        requested: vec![sequence(1, 100)],
        verified: vec![sequence(1, 20)],
        native_gaps: vec![sequence(21, 30)],
        unknown: vec![sequence(31, 70)],
        unreadable: vec![sequence(71, 100)],
        ordering: EventOrdering::NativeSequence,
        event_clock: Some(EventClock {
            basis: EventClockBasis::TimezoneUnknown,
            uncertainty_nanoseconds: None,
        }),
        observation_clock: Some(EventClock {
            basis: EventClockBasis::FixedOffset { seconds: 8 * 3600 },
            uncertainty_nanoseconds: Some(1000),
        }),
        receipt_clock: Some(EventClock {
            basis: EventClockBasis::Utc,
            uncertainty_nanoseconds: Some(10),
        }),
    }
}

fn runtime() -> RuntimeCoverage {
    let process = RuntimeComponent::Process {
        process_id: "pid-42".into(),
    };
    let region = RuntimeComponent::MemoryRegion {
        process_id: "pid-42".into(),
        range: range(4096, 4096),
    };
    let mounts = RuntimeComponent::NativeState {
        native_kind: "mounts".into(),
        native_id: "mount-ns-7".into(),
    };
    let sockets = RuntimeComponent::NativeState {
        native_kind: "sockets".into(),
        native_id: "net-ns-9".into(),
    };
    RuntimeCoverage {
        context: context(),
        snapshot_id: "captured-snapshot-1".into(),
        requested: [
            process.clone(),
            region.clone(),
            mounts.clone(),
            sockets.clone(),
        ]
        .into_iter()
        .collect(),
        captured: [process].into_iter().collect(),
        not_captured: [region].into_iter().collect(),
        unavailable: [mounts].into_iter().collect(),
        unknown: [sockets].into_iter().collect(),
    }
}

fn complete_bytes() -> TargetCoverage {
    TargetCoverage::Bytes(ByteCoverage {
        verified: vec![range(0, 16)],
        native_holes: Vec::new(),
        unknown: Vec::new(),
        unreadable: Vec::new(),
        ..bytes()
    })
}

fn unknown_bytes() -> TargetCoverage {
    TargetCoverage::Bytes(ByteCoverage {
        verified: Vec::new(),
        native_holes: Vec::new(),
        unknown: vec![range(0, 16)],
        unreadable: Vec::new(),
        ..bytes()
    })
}

fn result(status: TargetStatus, error: Option<TargetFailure>) -> TargetResult {
    TargetResult {
        target_id: "file-7".into(),
        status,
        required_capabilities: [CapabilityKind::FileRange].into_iter().collect(),
        coverage: unknown_bytes(),
        evidence_ids: BTreeSet::new(),
        error,
    }
}

#[test]
fn every_coverage_space_roundtrips_without_losing_native_metadata() {
    for value in [
        TargetCoverage::Bytes(bytes()),
        TargetCoverage::Namespace(namespace()),
        TargetCoverage::Keys(keys()),
        TargetCoverage::Events(events()),
        TargetCoverage::Runtime(runtime()),
    ] {
        value.validate().unwrap();
        roundtrip(&value);
    }
    let encoded = serde_json::to_string(&TargetCoverage::Events(events())).unwrap();
    assert!(encoded.contains("9007199254740993"));
    assert!(encoded.contains("timezone_unknown"));
    assert!(encoded.contains("native-sequence-v1"));
}

#[test]
fn byte_coverage_partitions_request_and_counts_only_verified_portions() {
    let value = bytes();
    value.validate().unwrap();
    assert_eq!(value.verified_bytes().unwrap(), 4);
    assert_eq!(value.native_hole_bytes().unwrap(), 4);
    assert!(!TargetCoverage::Bytes(value.clone()).is_complete().unwrap());
    assert!(TargetCoverage::Bytes(value).has_proven_portion().unwrap());
    assert!(complete_bytes().is_complete().unwrap());
    assert!(!unknown_bytes().has_proven_portion().unwrap());
    let empty = ByteCoverage {
        target_size: 0,
        requested: Vec::new(),
        verified: Vec::new(),
        native_holes: Vec::new(),
        unknown: Vec::new(),
        unreadable: Vec::new(),
    };
    assert!(TargetCoverage::Bytes(empty.clone()).is_complete().unwrap());
    roundtrip(&empty);
}

#[test]
fn byte_coverage_rejects_overlaps_unclassified_bytes_and_ranges_outside_request() {
    let mut value = bytes();
    value.native_holes = vec![range(3, 5)];
    assert_eq!(
        value.validate(),
        Err(CoverageError::ContradictoryClassification)
    );
    let mut value = bytes();
    value.verified = vec![range(0, 3)];
    assert_eq!(value.validate(), Err(CoverageError::IncompletePartition));
    let mut value = bytes();
    value.requested = vec![range(0, 15)];
    assert_eq!(value.validate(), Err(CoverageError::IncompletePartition));
    let mut value = bytes();
    value.unknown = vec![range(8, 9)];
    assert_eq!(value.validate(), Err(CoverageError::OutOfBounds));
    let mut value = bytes();
    value.verified = vec![range(0, 4), range(0, 4)];
    assert_eq!(value.validate(), Err(CoverageError::NoncanonicalRanges));
    let mut value = bytes();
    value.requested.clear();
    assert_eq!(value.validate(), Err(CoverageError::EmptyRequest));
}

#[test]
fn byte_coverage_rejects_deserialized_overflow_without_panicking() {
    let mut encoded = serde_json::to_value(bytes()).unwrap();
    encoded["verified"] = json!([{"start": u64::MAX, "length": 1}]);
    assert!(serde_json::from_value::<ByteCoverage>(encoded).is_err());
    let mut encoded = serde_json::to_value(bytes()).unwrap();
    encoded["verified"] = json!([{"start": 0, "length": 0}]);
    assert!(serde_json::from_value::<ByteCoverage>(encoded).is_err());
    let mut encoded = serde_json::to_value(bytes()).unwrap();
    encoded["verified"] = json!([]);
    assert!(serde_json::from_value::<ByteCoverage>(encoded).is_err());
}

#[test]
fn byte_partition_handles_disjoint_requests_and_adjacent_request_boundaries() {
    let disjoint = ByteCoverage {
        target_size: 100,
        requested: vec![range(10, 10), range(70, 20)],
        verified: vec![range(10, 10)],
        native_holes: vec![range(70, 20)],
        unknown: Vec::new(),
        unreadable: Vec::new(),
    };
    assert!(TargetCoverage::Bytes(disjoint).is_complete().unwrap());
    let adjacent = ByteCoverage {
        requested: vec![range(0, 8), range(8, 8)],
        verified: vec![range(0, 16)],
        native_holes: Vec::new(),
        unknown: Vec::new(),
        unreadable: Vec::new(),
        ..bytes()
    };
    adjacent.validate().unwrap();
    let maximum = ByteCoverage {
        target_size: u64::MAX,
        requested: vec![range(0, u64::MAX)],
        verified: vec![range(0, u64::MAX)],
        native_holes: Vec::new(),
        unknown: Vec::new(),
        unreadable: Vec::new(),
    };
    assert_eq!(maximum.verified_bytes().unwrap(), u64::MAX);
    roundtrip(&maximum);
}

#[test]
fn namespace_completeness_requires_a_verified_native_eof() {
    let mut value = namespace();
    assert_eq!(value.verified_objects().unwrap(), 513);
    assert!(TargetCoverage::Namespace(value.clone())
        .is_complete()
        .unwrap());
    value.pages.pop();
    assert!(!TargetCoverage::Namespace(value.clone())
        .is_complete()
        .unwrap());
    assert_eq!(value.verified_objects().unwrap(), 512);
    value.pages.clear();
    assert!(!TargetCoverage::Namespace(value.clone())
        .is_complete()
        .unwrap());
    assert!(!TargetCoverage::Namespace(value)
        .has_proven_portion()
        .unwrap());
    let mut empty = namespace();
    empty.pages = vec![NamespacePage {
        input_cursor: None,
        first_ordinal: 0,
        object_count: 0,
        page_sha256: "c".repeat(64),
        verified: true,
        end: NamespacePageEnd::Eof {
            witness_sha256: "d".repeat(64),
        },
    }];
    assert_eq!(empty.verified_objects().unwrap(), 0);
    assert!(TargetCoverage::Namespace(empty.clone())
        .is_complete()
        .unwrap());
    roundtrip(&empty);
}

#[test]
fn namespace_interruption_preserves_only_previously_verified_page_counts() {
    let mut value = namespace();
    value.pages[1].verified = false;
    value.pages[1].end = NamespacePageEnd::More {
        cursor: "unverified-continuation".into(),
    };
    assert_eq!(value.verified_objects().unwrap(), 512);
    assert!(!TargetCoverage::Namespace(value.clone())
        .is_complete()
        .unwrap());
    roundtrip(&value);
    value.pages[1].input_cursor = Some("unverified-continuation".into());
    assert_eq!(value.validate(), Err(CoverageError::InvalidCursorChain));
}

#[test]
fn namespace_rejects_broken_repeated_and_unverified_cursor_chains() {
    let mut value = namespace();
    value.pages[1].input_cursor = None;
    assert_eq!(value.validate(), Err(CoverageError::InvalidCursorChain));
    let mut value = namespace();
    value.pages[1].end = NamespacePageEnd::More {
        cursor: "opaque-cursor-512".into(),
    };
    assert_eq!(value.validate(), Err(CoverageError::InvalidCursorChain));
    let mut value = namespace();
    value.pages[0].verified = false;
    assert_eq!(value.validate(), Err(CoverageError::UnverifiedContinuation));
    let mut value = namespace();
    value.pages[1].verified = false;
    assert_eq!(value.validate(), Err(CoverageError::InvalidEof));
    let mut value = namespace();
    value.pages[0].end = NamespacePageEnd::Eof {
        witness_sha256: "c".repeat(64),
    };
    assert_eq!(value.validate(), Err(CoverageError::InvalidEof));
}

#[test]
fn namespace_rejects_count_overflow_missing_ordinals_and_invalid_witnesses() {
    let mut value = namespace();
    value.pages[0].object_count = u64::MAX;
    value.pages[1].first_ordinal = u64::MAX;
    assert_eq!(value.validate(), Err(CoverageError::InvalidPageCount));
    let mut value = namespace();
    value.pages[1].first_ordinal += 1;
    assert_eq!(value.validate(), Err(CoverageError::InvalidPageCount));
    let mut value = namespace();
    value.pages[1].end = NamespacePageEnd::Eof {
        witness_sha256: "not-a-witness".into(),
    };
    assert!(matches!(
        value.validate(),
        Err(CoverageError::InvalidDigest { .. })
    ));
    assert!(
        serde_json::from_value::<NamespaceCoverage>(serde_json::to_value(value).unwrap()).is_err()
    );
}

#[test]
fn key_coverage_preserves_inclusive_tombstones_and_binary_native_keys() {
    let value = keys();
    value.validate().unwrap();
    assert!(!TargetCoverage::Keys(value.clone()).is_complete().unwrap());
    roundtrip(&value);
    let empty_key = key_range(before(&[]), after(&[]));
    let value = KeyCoverage {
        requested: vec![empty_key.clone()],
        verified: vec![empty_key],
        tombstones: Vec::new(),
        unknown: Vec::new(),
        unreadable: Vec::new(),
        ..keys()
    };
    assert!(TargetCoverage::Keys(value.clone()).is_complete().unwrap());
    roundtrip(&value);
    let binary = key_range(before(&[0, 255, 0]), after(&[0, 255, 0]));
    let value = KeyCoverage {
        requested: vec![binary.clone()],
        tombstones: vec![binary],
        verified: Vec::new(),
        ..value
    };
    value.validate().unwrap();
    roundtrip(&value);
}

#[test]
fn key_coverage_uses_adapter_sort_keys_and_refuses_inconsistent_key_bindings() {
    let lower = KeyBoundary::Before {
        native_key: b"z".to_vec(),
        sort_key: vec![0],
    };
    let upper = KeyBoundary::After {
        native_key: b"a".to_vec(),
        sort_key: vec![255],
    };
    let selected = key_range(lower, upper);
    let mut value = KeyCoverage {
        ordering: KeyOrdering {
            comparator_id: "native-reverse-bytewise".into(),
            sort_key_encoding_id: "reverse-order-sort-keys".into(),
            witness_sha256: "f".repeat(64),
        },
        requested: vec![selected.clone()],
        verified: vec![selected],
        tombstones: Vec::new(),
        unknown: Vec::new(),
        unreadable: Vec::new(),
        ..keys()
    };
    value.validate().unwrap();
    roundtrip(&value);
    value.verified[0].lower = before(b"z");
    assert_eq!(value.validate(), Err(CoverageError::InconsistentKeyOrder));
    assert!(serde_json::from_value::<KeyCoverage>(serde_json::to_value(value).unwrap()).is_err());
}

#[test]
fn key_coverage_rejects_gaps_overlaps_invalid_bounds_and_unwitnessed_order() {
    let mut value = keys();
    value.tombstones.clear();
    assert_eq!(value.validate(), Err(CoverageError::IncompletePartition));
    let mut value = keys();
    value.tombstones[0].lower = before(b"before-deleted");
    assert_eq!(
        value.validate(),
        Err(CoverageError::ContradictoryClassification)
    );
    let mut value = keys();
    value.verified[0] = key_range(after(b"z"), before(b"a"));
    assert_eq!(value.validate(), Err(CoverageError::InvalidRange));
    let mut value = keys();
    value.ordering.witness_sha256.clear();
    assert!(matches!(
        value.validate(),
        Err(CoverageError::InvalidDigest { .. })
    ));
}

#[test]
fn event_coverage_preserves_full_u64_native_sequences_and_all_orderings() {
    for ordering in [
        EventOrdering::NativeSequence,
        EventOrdering::EventTime,
        EventOrdering::ObservationTime,
        EventOrdering::ReceiptTime,
        EventOrdering::Unordered,
    ] {
        let value = EventCoverage {
            ordering,
            ..events()
        };
        value.validate().unwrap();
        roundtrip(&value);
    }
    let value = EventCoverage {
        requested: vec![sequence(0, u64::MAX)],
        verified: vec![sequence(0, u64::MAX)],
        native_gaps: Vec::new(),
        unknown: Vec::new(),
        unreadable: Vec::new(),
        ..events()
    };
    assert!(TargetCoverage::Events(value.clone()).is_complete().unwrap());
    roundtrip(&value);
}

#[test]
fn event_coverage_keeps_native_gaps_separate_from_unknown_and_unreadable_sequences() {
    assert!(!TargetCoverage::Events(events()).is_complete().unwrap());
    let mut value = events();
    value.verified = vec![sequence(1, 21)];
    assert_eq!(
        value.validate(),
        Err(CoverageError::ContradictoryClassification)
    );
    let mut value = events();
    value.native_gaps = vec![sequence(22, 30)];
    assert_eq!(value.validate(), Err(CoverageError::IncompletePartition));
    let mut value = events();
    value.requested = vec![sequence(2, 1)];
    assert_eq!(value.validate(), Err(CoverageError::InvalidRange));
    assert!(serde_json::from_value::<EventCoverage>(serde_json::to_value(value).unwrap()).is_err());
}

#[test]
fn event_clocks_roundtrip_uncertainty_and_validate_ordering_requirements() {
    for basis in [
        EventClockBasis::Utc,
        EventClockBasis::FixedOffset { seconds: -86399 },
        EventClockBasis::TimezoneUnknown,
        EventClockBasis::Monotonic {
            clock_id: "boot-7".into(),
        },
    ] {
        let clock = EventClock {
            basis,
            uncertainty_nanoseconds: Some(u64::MAX),
        };
        clock.validate().unwrap();
        roundtrip(&clock);
    }
    for seconds in [i32::MIN, -86400, 86400, i32::MAX] {
        assert_eq!(
            EventClock {
                basis: EventClockBasis::FixedOffset { seconds },
                uncertainty_nanoseconds: None,
            }
            .validate(),
            Err(CoverageError::InvalidClock)
        );
    }
    let value = EventCoverage {
        ordering: EventOrdering::EventTime,
        event_clock: None,
        ..events()
    };
    assert_eq!(value.validate(), Err(CoverageError::MissingClock));
}

#[test]
fn runtime_coverage_never_infers_uncaptured_state_from_a_captured_process() {
    let mut value = runtime();
    value.validate().unwrap();
    assert!(!TargetCoverage::Runtime(value.clone())
        .is_complete()
        .unwrap());
    for component in &value.requested {
        roundtrip(component);
    }
    value.captured = value.requested.clone();
    value.not_captured.clear();
    value.unavailable.clear();
    value.unknown.clear();
    assert!(TargetCoverage::Runtime(value.clone())
        .is_complete()
        .unwrap());
    roundtrip(&value);
}

#[test]
fn runtime_coverage_rejects_duplicate_classifications_missing_components_and_regions() {
    let mut value = runtime();
    value.not_captured.extend(value.captured.clone());
    assert_eq!(
        value.validate(),
        Err(CoverageError::ContradictoryClassification)
    );
    let mut value = runtime();
    value.unknown.clear();
    assert_eq!(value.validate(), Err(CoverageError::IncompletePartition));
    let mut value = runtime();
    let overlapping = RuntimeComponent::MemoryRegion {
        process_id: "pid-42".into(),
        range: range(6000, 1000),
    };
    value.requested.insert(overlapping.clone());
    value.not_captured.insert(overlapping);
    assert_eq!(value.validate(), Err(CoverageError::NoncanonicalRanges));
    let mut encoded = serde_json::to_value(runtime()).unwrap();
    encoded["requested"] = json!([{
        "kind": "memory_region",
        "processId": "pid-42",
        "range": {"start": u64::MAX, "length": 1}
    }]);
    assert!(serde_json::from_value::<RuntimeCoverage>(encoded).is_err());
}

#[test]
fn all_capability_kinds_roundtrip_with_their_precise_coverage_spaces() {
    for kind in [
        CapabilityKind::AcquiredBytes,
        CapabilityKind::ContainerMetadata,
        CapabilityKind::MemberMetadata,
        CapabilityKind::BlockRange,
        CapabilityKind::DeviceSet,
        CapabilityKind::CommittedTree,
        CapabilityKind::Namespace,
        CapabilityKind::FileRange,
        CapabilityKind::ObjectInventory,
        CapabilityKind::ObjectRange,
        CapabilityKind::KvInventory,
        CapabilityKind::KeyRange,
        CapabilityKind::LogFrames,
        CapabilityKind::EventRange,
        CapabilityKind::ResourceRecords,
        CapabilityKind::RuntimeState,
    ] {
        let coverage = match kind {
            CapabilityKind::Namespace
            | CapabilityKind::DeviceSet
            | CapabilityKind::CommittedTree
            | CapabilityKind::ObjectInventory
            | CapabilityKind::ResourceRecords => TargetCoverage::Namespace(namespace()),
            CapabilityKind::KvInventory | CapabilityKind::KeyRange => TargetCoverage::Keys(keys()),
            CapabilityKind::EventRange => TargetCoverage::Events(events()),
            CapabilityKind::RuntimeState => TargetCoverage::Runtime(runtime()),
            _ => complete_bytes(),
        };
        assert!(kind.accepts(&coverage));
        let capability = CapabilityRef::new(
            "target-1".into(),
            "view-1".into(),
            kind,
            1,
            content_verified(),
            coverage,
        )
        .unwrap();
        capability.validate().unwrap();
        roundtrip(&capability);
        roundtrip(&kind);
    }
}

#[test]
fn capability_identity_binds_revision_integrity_view_and_proof() {
    let capability = CapabilityRef::new(
        "target-1".into(),
        "view-1".into(),
        CapabilityKind::FileRange,
        u64::MAX,
        content_verified(),
        complete_bytes(),
    )
    .unwrap();
    roundtrip(&capability);
    let mut changed = capability.clone();
    changed.revision -= 1;
    assert!(matches!(
        changed.validate(),
        Err(CapabilityError::IdentityMismatch)
    ));
    let encoded = serde_json::to_value(changed).unwrap();
    assert!(serde_json::from_value::<CapabilityRef>(encoded).is_err());
    let mut changed = capability.clone();
    changed.coverage = unknown_bytes();
    assert!(matches!(
        changed.validate(),
        Err(CapabilityError::IdentityMismatch)
    ));
    let other_revision = CapabilityRef::new(
        capability.target_id.clone(),
        capability.view_id.clone(),
        capability.kind,
        u64::MAX - 1,
        capability.integrity.clone(),
        capability.coverage.clone(),
    )
    .unwrap();
    assert_ne!(capability.id, other_revision.id);
}

#[test]
fn capability_rejects_wrong_views_wrong_spaces_and_unverified_proven_portions() {
    let create = |view: &str, kind, integrity, coverage| {
        CapabilityRef::new("target-1".into(), view.into(), kind, 1, integrity, coverage)
    };
    assert!(matches!(
        create(
            "view-2",
            CapabilityKind::Namespace,
            content_verified(),
            TargetCoverage::Namespace(namespace())
        ),
        Err(CapabilityError::ViewMismatch)
    ));
    assert!(matches!(
        create(
            "view-1",
            CapabilityKind::FileRange,
            content_verified(),
            TargetCoverage::Events(events())
        ),
        Err(CapabilityError::IncompatibleCoverage)
    ));
    assert!(matches!(
        create(
            "view-1",
            CapabilityKind::FileRange,
            CapabilityIntegrity::Observed,
            complete_bytes()
        ),
        Err(CapabilityError::UnverifiedCoverage)
    ));
    let provisional = create(
        "view-1",
        CapabilityKind::FileRange,
        CapabilityIntegrity::Observed,
        unknown_bytes(),
    )
    .unwrap();
    roundtrip(&provisional);
    assert!(matches!(
        CapabilityRef::new(
            "target-1".into(),
            "view-1".into(),
            CapabilityKind::FileRange,
            0,
            content_verified(),
            complete_bytes(),
        ),
        Err(CapabilityError::InvalidRevision)
    ));
}

#[test]
fn activity_states_roundtrip_and_require_admission_before_execution() {
    let mut state = ActivityState::Pending;
    for next in [
        ActivityState::Ready,
        ActivityState::Admitted,
        ActivityState::Running,
        ActivityState::Succeeded,
    ] {
        state = state.transition(next).unwrap();
        roundtrip(&state);
    }
    assert!(state.is_terminal());
    assert!(!ActivityState::Ready.is_terminal());
    assert!(ActivityState::Pending
        .transition(ActivityState::Running)
        .is_err());
    assert!(ActivityState::Running
        .transition(ActivityState::Ready)
        .is_err());
    for state in [
        ActivityState::Succeeded,
        ActivityState::Failed,
        ActivityState::Blocked,
        ActivityState::Cancelled,
    ] {
        assert!(state.is_terminal());
        roundtrip(&state);
        assert!(state.transition(ActivityState::Ready).is_err());
    }
    roundtrip(&ActivityState::Pending);
}

#[test]
fn independent_target_results_allow_success_beside_missing_evidence() {
    let verified = TargetResult {
        coverage: complete_bytes(),
        evidence_ids: ["receipt-7".into()].into_iter().collect(),
        ..result(TargetStatus::Verified, None)
    };
    let needs = result(
        TargetStatus::NeedsEvidence,
        Some(TargetFailure::MissingEvidence {
            requirement: EvidenceRequirement::ReferencedContent,
        }),
    );
    verified.validate().unwrap();
    needs.validate().unwrap();
    roundtrip(&vec![verified, needs]);
    let partial = TargetResult {
        coverage: TargetCoverage::Bytes(bytes()),
        evidence_ids: ["receipt-range-0-4".into()].into_iter().collect(),
        ..result(TargetStatus::Partial, Some(TargetFailure::IoUnavailable))
    };
    partial.validate().unwrap();
    roundtrip(&partial);
}

#[test]
fn target_statuses_roundtrip_with_corresponding_safe_failures() {
    for (status, error) in [
        (
            TargetStatus::NeedsEvidence,
            TargetFailure::MissingCapabilities {
                capabilities: [CapabilityKind::FileRange].into_iter().collect(),
            },
        ),
        (
            TargetStatus::UnsupportedFeature,
            TargetFailure::UnsupportedFeature {
                capability: CapabilityKind::FileRange,
            },
        ),
        (
            TargetStatus::CorruptMetadata,
            TargetFailure::CorruptMetadata {
                reason: MetadataFailure::InvalidStructure,
            },
        ),
        (
            TargetStatus::Conflicted,
            TargetFailure::Conflicted {
                reason: ConflictReason::NativeVersion,
            },
        ),
        (
            TargetStatus::Cancelled,
            TargetFailure::Cancelled {
                scope: CancellationScope::Target,
            },
        ),
    ] {
        let value = result(status, Some(error));
        value.validate().unwrap();
        roundtrip(&value);
        roundtrip(&status);
    }
}

#[test]
fn target_results_reject_false_completion_and_inconsistent_partial_or_failure_status() {
    assert!(matches!(
        result(TargetStatus::Verified, None).validate(),
        Err(OutcomeError::InconsistentTargetStatus)
    ));
    let no_evidence = TargetResult {
        coverage: complete_bytes(),
        ..result(TargetStatus::Verified, None)
    };
    assert_eq!(
        no_evidence.validate(),
        Err(OutcomeError::MissingEvidenceReferences)
    );
    let partial_without_progress = result(TargetStatus::Partial, None);
    assert_eq!(
        partial_without_progress.validate(),
        Err(OutcomeError::InconsistentTargetStatus)
    );
    let partial_full = TargetResult {
        coverage: complete_bytes(),
        evidence_ids: ["receipt-7".into()].into_iter().collect(),
        ..result(TargetStatus::Partial, None)
    };
    assert_eq!(
        partial_full.validate(),
        Err(OutcomeError::InconsistentTargetStatus)
    );
    let wrong = result(
        TargetStatus::Cancelled,
        Some(TargetFailure::CorruptMetadata {
            reason: MetadataFailure::InvalidChecksum,
        }),
    );
    assert_eq!(
        wrong.validate(),
        Err(OutcomeError::InconsistentTargetStatus)
    );
    assert!(serde_json::from_value::<TargetResult>(serde_json::to_value(wrong).unwrap()).is_err());
}

#[test]
fn partial_results_require_relevant_missing_capabilities_and_preserve_failure_statuses() {
    let partial = TargetResult {
        coverage: TargetCoverage::Bytes(bytes()),
        evidence_ids: ["receipt-range-0-4".into()].into_iter().collect(),
        ..result(TargetStatus::Partial, None)
    };
    partial.validate().unwrap();
    let missing_required = TargetResult {
        error: Some(TargetFailure::MissingCapabilities {
            capabilities: [CapabilityKind::FileRange].into_iter().collect(),
        }),
        ..partial.clone()
    };
    missing_required.validate().unwrap();
    roundtrip(&missing_required);
    for error in [
        TargetFailure::MissingCapabilities {
            capabilities: [CapabilityKind::RuntimeState].into_iter().collect(),
        },
        TargetFailure::CorruptMetadata {
            reason: MetadataFailure::InvalidChecksum,
        },
        TargetFailure::Conflicted {
            reason: ConflictReason::NativeVersion,
        },
        TargetFailure::Cancelled {
            scope: CancellationScope::Target,
        },
        TargetFailure::UnsupportedFeature {
            capability: CapabilityKind::FileRange,
        },
    ] {
        let invalid = TargetResult {
            error: Some(error),
            ..partial.clone()
        };
        assert_eq!(
            invalid.validate(),
            Err(OutcomeError::InconsistentTargetStatus)
        );
        assert!(
            serde_json::from_value::<TargetResult>(serde_json::to_value(invalid).unwrap()).is_err()
        );
    }
}

#[test]
fn target_failures_roundtrip_all_typed_reasons_without_arbitrary_error_text() {
    for requirement in [
        EvidenceRequirement::Member,
        EvidenceRequirement::BackingVersion,
        EvidenceRequirement::NativeVersionWitness,
        EvidenceRequirement::NamespacePage,
        EvidenceRequirement::ReferencedContent,
        EvidenceRequirement::RuntimeSnapshot,
        EvidenceRequirement::DecryptionMaterial,
    ] {
        roundtrip(&TargetFailure::MissingEvidence { requirement });
    }
    for reason in [
        MetadataFailure::InvalidStructure,
        MetadataFailure::InvalidChecksum,
        MetadataFailure::Truncated,
        MetadataFailure::InvalidNativeOrder,
        MetadataFailure::InconsistentReferences,
    ] {
        roundtrip(&TargetFailure::CorruptMetadata { reason });
    }
    for reason in [
        ConflictReason::MemberIdentity,
        ConflictReason::NativeVersion,
        ConflictReason::InputChanged,
        ConflictReason::PublicationOwner,
    ] {
        roundtrip(&TargetFailure::Conflicted { reason });
    }
    for scope in [
        CancellationScope::Target,
        CancellationScope::Run,
        CancellationScope::Source,
        CancellationScope::Case,
    ] {
        roundtrip(&TargetFailure::Cancelled { scope });
    }
    for resource in [
        ResourceKind::Bytes,
        ResourceKind::Readers,
        ResourceKind::Handles,
        ResourceKind::Cpu,
        ResourceKind::ActiveWork,
        ResourceKind::DependencyDepth,
    ] {
        let failure = TargetFailure::ResourceLimit {
            resource,
            required: u64::MAX,
            limit: 10,
        };
        failure.validate().unwrap();
        roundtrip(&failure);
    }
    for error in [
        TargetFailure::IoUnavailable,
        TargetFailure::DeadlineExceeded,
        TargetFailure::RequiredDependencyFailed,
    ] {
        roundtrip(&error);
    }
    assert!(serde_json::from_value::<TargetFailure>(json!({
        "kind": "io_unavailable",
        "path": "D:\\evidence\\private.raw",
        "message": "raw parser failure"
    }))
    .is_err());
}

#[test]
fn typed_failures_validate_resource_limits_and_missing_capability_sets() {
    assert_eq!(
        TargetFailure::MissingCapabilities {
            capabilities: BTreeSet::new()
        }
        .validate(),
        Err(OutcomeError::EmptyMissingCapabilities)
    );
    assert_eq!(
        TargetFailure::ResourceLimit {
            resource: ResourceKind::Bytes,
            required: 10,
            limit: 10
        }
        .validate(),
        Err(OutcomeError::InvalidResourceLimit)
    );
    let unrelated = result(
        TargetStatus::NeedsEvidence,
        Some(TargetFailure::MissingCapabilities {
            capabilities: [CapabilityKind::RuntimeState].into_iter().collect(),
        }),
    );
    assert_eq!(
        unrelated.validate(),
        Err(OutcomeError::InconsistentTargetStatus)
    );
}

#[test]
fn serialized_records_reject_unknown_fields_and_missing_required_native_metadata() {
    let mut encoded = serde_json::to_value(TargetCoverage::Namespace(namespace())).unwrap();
    encoded["unexpected"] = json!(true);
    assert!(serde_json::from_value::<TargetCoverage>(encoded).is_err());
    let mut encoded = serde_json::to_value(TargetCoverage::Keys(keys())).unwrap();
    encoded["coverage"]["ordering"]["comparator_id"] = json!("old-alias");
    assert!(serde_json::from_value::<TargetCoverage>(encoded).is_err());
    let mut encoded = serde_json::to_value(TargetCoverage::Events(events())).unwrap();
    encoded["coverage"]["context"]["version"]
        .as_object_mut()
        .unwrap()
        .remove("nativeValue");
    assert!(serde_json::from_value::<TargetCoverage>(encoded).is_err());
    for value in [
        json!({"kind": "utc", "message": "unexpected"}),
        json!({"kind": "timezone_unknown", "path": "unexpected"}),
    ] {
        assert!(serde_json::from_value::<EventClockBasis>(value).is_err());
    }
    assert!(serde_json::from_value::<KeyBoundary>(json!({
        "kind": "unbounded_start",
        "message": "unexpected"
    }))
    .is_err());
    let mut encoded = serde_json::to_value(context()).unwrap();
    encoded["version"]["witnessSha256"] = json!("invalid");
    assert!(serde_json::from_value::<CoverageContext>(encoded).is_err());
}
