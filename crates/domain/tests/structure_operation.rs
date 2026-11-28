use std::collections::BTreeSet;

use domain::structure::{
    AcquisitionId, AddressSpace, BackingVersionRef, ByteRange, NativeAlgorithmRef,
    NativeMappingRef, OperationInput, OperationModelError, OrderingRequirement, ReadOperation,
    ReadOperationNode, ReadOperationPlan, ReadVerification, SourceCaptureRef, SourceRevisionRef,
    ViewModelError, WorkIdentity,
};
use sha2::{Digest, Sha256};

fn range() -> ByteRange {
    ByteRange::new(0, 4096).unwrap()
}

fn input(id: &str) -> OperationInput {
    OperationInput {
        operation_id: id.into(),
        role: "data".into(),
        range: range(),
    }
}

fn backing() -> BackingVersionRef {
    BackingVersionRef::new(
        SourceCaptureRef::new(
            SourceRevisionRef {
                case_id: "case".into(),
                source_id: "source".into(),
                revision: u64::MAX,
            },
            AcquisitionId("capture".into()),
        )
        .unwrap(),
        "instance".into(),
        Some("a".repeat(64)),
        "change-token".into(),
    )
    .unwrap()
}

fn mapping() -> NativeMappingRef {
    NativeMappingRef {
        view_id: "view".into(),
        adapter_id: "adapter".into(),
        adapter_version: "1".into(),
        mapping_sha256: "a".repeat(64),
        version_witness_sha256: "b".repeat(64),
    }
}

fn algorithm() -> NativeAlgorithmRef {
    NativeAlgorithmRef {
        id: "native-algorithm".into(),
        version: "1".into(),
        parameters_sha256: "a".repeat(64),
        version_witness_sha256: "b".repeat(64),
    }
}

#[test]
fn every_operation_round_trips_its_native_dependencies() {
    let operations = vec![
        ReadOperation::ReadBackingRange {
            backing: backing(),
            range: range(),
        },
        ReadOperation::Window {
            input: input("parent"),
            output_range: range(),
        },
        ReadOperation::MapExtents {
            mapping: mapping(),
            inputs: vec![input("parent")],
        },
        ReadOperation::Interleave {
            mapping: mapping(),
            inputs: vec![input("first"), input("second")],
        },
        ReadOperation::SelectReplica {
            selected: input("selected"),
            witness_sha256: "a".repeat(64),
        },
        ReadOperation::RecoverRange {
            helpers: vec![input("helper")],
            algorithm: algorithm(),
        },
        ReadOperation::ResolveReference {
            mapping: mapping(),
            inputs: vec![input("parent")],
        },
        ReadOperation::DecodeContent {
            inputs: vec![input("compressed")],
            codec: algorithm(),
            transform_identity: "transform".into(),
        },
        ReadOperation::ApplyOverlay {
            ordered_layers: vec![input("base"), input("delta")],
            algorithm: algorithm(),
        },
        ReadOperation::LookupNamespace {
            namespace_snapshot: "snapshot".into(),
            native_id: "inode".into(),
            inputs: vec![input("namespace")],
        },
        ReadOperation::VerifyUnit {
            input: input("data"),
            method: ReadVerification::ContentDigest,
            expected_sha256: Some("a".repeat(64)),
        },
    ];
    for operation in operations {
        operation.validate().unwrap();
        let encoded = serde_json::to_string(&operation).unwrap();
        assert_eq!(
            serde_json::from_str::<ReadOperation>(&encoded).unwrap(),
            operation
        );
    }
}

fn plan() -> ReadOperationPlan {
    ReadOperationPlan {
        view_id: "view".into(),
        target_id: "target".into(),
        root_operation_id: "window".into(),
        operations: vec![
            ReadOperationNode {
                id: "bytes".into(),
                output_space: AddressSpace::PhysicalByte,
                output_range: range(),
                operation: ReadOperation::ReadBackingRange {
                    backing: backing(),
                    range: range(),
                },
            },
            ReadOperationNode {
                id: "window".into(),
                output_space: AddressSpace::LogicalBlock,
                output_range: range(),
                operation: ReadOperation::Window {
                    input: input("bytes"),
                    output_range: range(),
                },
            },
        ],
    }
}

#[test]
fn operation_plan_validates_dependencies_cycles_and_actual_input_bounds() {
    let valid = plan();
    valid.validate().unwrap();
    assert_eq!(
        serde_json::from_str::<ReadOperationPlan>(&serde_json::to_string(&valid).unwrap()).unwrap(),
        valid
    );
    let mut invalid = valid.clone();
    invalid.operations[1].operation = ReadOperation::Window {
        input: input("missing"),
        output_range: range(),
    };
    assert!(matches!(
        invalid.validate(),
        Err(OperationModelError::MissingDependency)
    ));
    invalid.operations[1].operation = ReadOperation::Window {
        input: input("window"),
        output_range: range(),
    };
    assert!(matches!(
        invalid.validate(),
        Err(OperationModelError::DependencyCycle)
    ));
    invalid = valid;
    invalid.operations[0].output_range = ByteRange::new(0, 1).unwrap();
    assert!(matches!(
        invalid.validate(),
        Err(OperationModelError::InvalidRange)
    ));
}

#[test]
fn work_identity_normalizes_input_sets_and_preserves_ordering() {
    let work = WorkIdentity {
        operation_id: "inspect".into(),
        adapter_id: "adapter".into(),
        adapter_version: "1".into(),
        goal_id: "goal".into(),
        input_capability_versions: BTreeSet::from(["a".into(), "b".into()]),
        selector_sha256: "a".repeat(64),
        version_witness_sha256: "b".repeat(64),
        scope_witness_sha256: "c".repeat(64),
        ordering: OrderingRequirement::Unordered,
        policy_version: "1".into(),
    };
    let mut reordered = work.clone();
    reordered.input_capability_versions =
        ["b".into(), "a".into(), "a".into()].into_iter().collect();
    assert_eq!(work.key().unwrap(), reordered.key().unwrap());
    reordered.ordering = OrderingRequirement::NativeSequence {
        domain: "journal".into(),
        boundary_sha256: "d".repeat(64),
    };
    assert_ne!(work.key().unwrap(), reordered.key().unwrap());
    assert_eq!(
        serde_json::from_str::<WorkIdentity>(&serde_json::to_string(&work).unwrap()).unwrap(),
        work
    );
}

#[test]
fn invalid_serialized_byte_ranges_cannot_enter_operation_plans() {
    assert!(
        serde_json::from_str::<ByteRange>(r#"{"start":18446744073709551615,"length":1}"#).is_err()
    );
    assert!(serde_json::from_str::<domain::structure::ByteRangeSet>(
        r#"{"targetSize":16,"ranges":[{"start":15,"length":2}]}"#
    )
    .is_err());
}

#[test]
fn plan_deserialization_rejects_missing_dependencies_cycles_duplicates_and_unused_nodes() {
    let original = plan();
    let mut cases = vec![];
    let mut invalid = original.clone();
    invalid.operations[1].operation = ReadOperation::Window {
        input: input("missing"),
        output_range: range(),
    };
    cases.push(invalid);
    let mut invalid = original.clone();
    invalid.operations[1].operation = ReadOperation::Window {
        input: input("window"),
        output_range: range(),
    };
    cases.push(invalid);
    let mut invalid = original.clone();
    invalid.operations.push(invalid.operations[0].clone());
    cases.push(invalid);
    let mut invalid = original.clone();
    let mut unused = invalid.operations[0].clone();
    unused.id = "unused".into();
    invalid.operations.push(unused);
    cases.push(invalid);
    let mut invalid = original.clone();
    invalid.operations.clear();
    cases.push(invalid);
    let mut invalid = original;
    invalid.root_operation_id = "missing".into();
    cases.push(invalid);
    for invalid in cases {
        assert!(invalid.validate().is_err());
        assert_rejected_plan(&invalid);
    }
}

#[test]
fn plan_and_work_deserialization_reject_unknown_and_required_missing_fields() {
    let mut serialized = serde_json::to_value(plan()).unwrap();
    serialized["unknownField"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ReadOperationPlan>(serialized).is_err());
    for field in ["viewId", "targetId", "rootOperationId", "operations"] {
        let mut serialized = serde_json::to_value(plan()).unwrap();
        serialized.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ReadOperationPlan>(serialized).is_err());
    }
    let mut serialized = serde_json::to_value(work()).unwrap();
    serialized["unknownField"] = serde_json::json!(true);
    assert!(serde_json::from_value::<WorkIdentity>(serialized).is_err());
    for field in [
        "operationId",
        "adapterId",
        "adapterVersion",
        "goalId",
        "inputCapabilityVersions",
        "selectorSha256",
        "versionWitnessSha256",
        "scopeWitnessSha256",
        "ordering",
        "policyVersion",
    ] {
        let mut serialized = serde_json::to_value(work()).unwrap();
        serialized.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<WorkIdentity>(serialized).is_err());
    }
}

#[test]
fn backing_operation_uses_the_full_backing_version_validation() {
    let original = backing();
    let mut cases = vec![original; 4];
    cases[0].capture.source.revision = 0;
    cases[1].change_token = " ".into();
    cases[2].instance_id = " ".into();
    cases[3].content_manifest_sha256 = Some("A".repeat(64));
    for backing in cases {
        let operation = ReadOperation::ReadBackingRange {
            backing,
            range: range(),
        };
        assert!(matches!(
            operation.validate(),
            Err(OperationModelError::Backing(_))
        ));
    }
    let mut invalid = plan();
    if let ReadOperation::ReadBackingRange { backing, .. } = &mut invalid.operations[0].operation {
        backing.capture.source.revision = 0;
    }
    assert!(matches!(
        invalid.validate(),
        Err(OperationModelError::Backing(
            ViewModelError::UnregisteredRevision
        ))
    ));
    assert_rejected_plan(&invalid);
}

#[test]
fn backing_leaf_cannot_claim_bytes_outside_its_actual_read() {
    let mut leaf = plan();
    leaf.operations.pop();
    leaf.root_operation_id = "bytes".into();
    leaf.validate().unwrap();
    for claimed in [
        ByteRange::new(0, 4095).unwrap(),
        ByteRange::new(0, 4097).unwrap(),
        ByteRange::new(1, 4096).unwrap(),
    ] {
        let mut invalid = leaf.clone();
        invalid.operations[0].output_range = claimed;
        assert!(matches!(
            invalid.validate(),
            Err(OperationModelError::InvalidRange)
        ));
        assert_rejected_plan(&invalid);
    }
}

#[test]
fn backing_leaf_produces_bytes_in_its_physical_address_space() {
    for space in [
        AddressSpace::LogicalBlock,
        AddressSpace::File,
        AddressSpace::Object,
        AddressSpace::Key,
        AddressSpace::Namespace,
        AddressSpace::Event,
        AddressSpace::RuntimeState,
    ] {
        let mut invalid = plan();
        invalid.operations[0].output_space = space;
        assert!(matches!(
            invalid.validate(),
            Err(OperationModelError::InvalidOutputSpace)
        ));
        assert_rejected_plan(&invalid);
    }
}

#[test]
fn window_range_translation_matches_an_independent_eight_byte_example() {
    let physical_bytes = *b"ABCDEFGH";
    let original = translated_window_plan();
    original.validate().unwrap();
    assert_eq!(
        serde_json::from_str::<ReadOperationPlan>(&serde_json::to_string(&original).unwrap())
            .unwrap(),
        original
    );
    let selected = &original.operations[1];
    assert_eq!(selected.output_range, ByteRange::new(0, 4).unwrap());
    if let ReadOperation::Window {
        input,
        output_range,
    } = &selected.operation
    {
        assert_eq!(input.range, ByteRange::new(1026, 4).unwrap());
        assert_eq!(*output_range, ByteRange::new(0, 4).unwrap());
        let parent_start = original.operations[0].output_range.start();
        let start = usize::try_from(input.range.start() - parent_start).unwrap();
        let end = usize::try_from(input.range.end() - parent_start).unwrap();
        assert_eq!(&physical_bytes[start..end], b"CDEF");
        assert_eq!(output_range.length(), 4);
    } else {
        panic!("expected the window operation");
    }
}

#[test]
fn window_declared_output_cannot_create_or_discard_input_bytes() {
    for length in [3, 5] {
        let mut invalid = translated_window_plan();
        let claimed = ByteRange::new(0, length).unwrap();
        invalid.operations[1].output_range = claimed;
        if let ReadOperation::Window { output_range, .. } = &mut invalid.operations[1].operation {
            *output_range = claimed;
        }
        assert!(matches!(
            invalid.validate(),
            Err(OperationModelError::InvalidRange)
        ));
        assert_rejected_plan(&invalid);
    }
}

#[test]
fn window_node_cannot_override_the_declared_translated_output() {
    let original = translated_window_plan();
    for claimed in [
        ByteRange::new(0, 5).unwrap(),
        ByteRange::new(0, 3).unwrap(),
        ByteRange::new(1, 4).unwrap(),
    ] {
        let mut invalid = original.clone();
        invalid.operations[1].output_range = claimed;
        assert!(matches!(
            invalid.validate(),
            Err(OperationModelError::InvalidRange)
        ));
        assert_rejected_plan(&invalid);
    }
}

#[test]
fn window_translation_checks_both_ends_of_the_selected_parent_range() {
    for selected in [
        ByteRange::new(1023, 4).unwrap(),
        ByteRange::new(1029, 4).unwrap(),
    ] {
        let mut invalid = translated_window_plan();
        if let ReadOperation::Window { input, .. } = &mut invalid.operations[1].operation {
            input.range = selected;
        }
        assert!(matches!(
            invalid.validate(),
            Err(OperationModelError::InvalidRange)
        ));
        assert_rejected_plan(&invalid);
    }
}

#[test]
fn window_translation_preserves_exact_u64_endpoints_and_empty_boundary_ranges() {
    for (start, length) in [(u64::MAX - 4, 4), (u64::MAX, 0)] {
        let physical = ByteRange::new(start, length).unwrap();
        let logical = ByteRange::new(0, length).unwrap();
        let mut valid = plan();
        valid.operations[0].output_range = physical;
        valid.operations[0].operation = ReadOperation::ReadBackingRange {
            backing: backing(),
            range: physical,
        };
        valid.operations[1].output_range = logical;
        valid.operations[1].operation = ReadOperation::Window {
            input: OperationInput {
                range: physical,
                ..input("bytes")
            },
            output_range: logical,
        };
        valid.validate().unwrap();
        assert_eq!(
            serde_json::from_str::<ReadOperationPlan>(&serde_json::to_string(&valid).unwrap())
                .unwrap(),
            valid
        );
    }
}

#[test]
fn verification_and_replica_selection_preserve_the_selected_input_coordinates() {
    for operation in [
        ReadOperation::VerifyUnit {
            input: OperationInput {
                range: ByteRange::new(1026, 4).unwrap(),
                ..input("bytes")
            },
            method: ReadVerification::ContentDigest,
            expected_sha256: Some(hex::encode(Sha256::digest(b"CDEF"))),
        },
        ReadOperation::SelectReplica {
            selected: OperationInput {
                range: ByteRange::new(1026, 4).unwrap(),
                ..input("bytes")
            },
            witness_sha256: "a".repeat(64),
        },
    ] {
        let mut valid = translated_window_plan();
        valid.operations[1].operation = operation;
        valid.operations[1].output_space = AddressSpace::PhysicalByte;
        valid.operations[1].output_range = ByteRange::new(1026, 4).unwrap();
        valid.validate().unwrap();
        assert_eq!(
            serde_json::from_str::<ReadOperationPlan>(&serde_json::to_string(&valid).unwrap())
                .unwrap(),
            valid
        );
        for claimed in [
            ByteRange::new(0, 4).unwrap(),
            ByteRange::new(1026, 3).unwrap(),
            ByteRange::new(1026, 5).unwrap(),
        ] {
            let mut invalid = valid.clone();
            invalid.operations[1].output_range = claimed;
            assert!(matches!(
                invalid.validate(),
                Err(OperationModelError::InvalidRange)
            ));
            assert_rejected_plan(&invalid);
        }
        let mut invalid = valid;
        invalid.operations[1].output_space = AddressSpace::File;
        assert!(matches!(
            invalid.validate(),
            Err(OperationModelError::InvalidOutputSpace)
        ));
        assert_rejected_plan(&invalid);
    }
}

#[test]
fn mapping_operations_cannot_reference_a_different_view() {
    for operation in [
        ReadOperation::MapExtents {
            mapping: mapping(),
            inputs: vec![input("bytes")],
        },
        ReadOperation::Interleave {
            mapping: mapping(),
            inputs: vec![input("bytes")],
        },
        ReadOperation::ResolveReference {
            mapping: mapping(),
            inputs: vec![input("bytes")],
        },
    ] {
        let mut valid = plan();
        valid.operations[1].operation = operation;
        valid.validate().unwrap();
        valid.view_id = "other-view".into();
        assert!(matches!(
            valid.validate(),
            Err(OperationModelError::MappingViewMismatch)
        ));
        assert_rejected_plan(&valid);
    }
}

#[test]
fn identical_input_references_cannot_be_counted_twice() {
    for operation in [
        ReadOperation::MapExtents {
            mapping: mapping(),
            inputs: vec![input("bytes"), input("bytes")],
        },
        ReadOperation::Interleave {
            mapping: mapping(),
            inputs: vec![input("bytes"), input("bytes")],
        },
        ReadOperation::RecoverRange {
            algorithm: algorithm(),
            helpers: vec![input("bytes"), input("bytes")],
        },
        ReadOperation::ResolveReference {
            mapping: mapping(),
            inputs: vec![input("bytes"), input("bytes")],
        },
        ReadOperation::DecodeContent {
            codec: algorithm(),
            transform_identity: "transform".into(),
            inputs: vec![input("bytes"), input("bytes")],
        },
        ReadOperation::ApplyOverlay {
            algorithm: algorithm(),
            ordered_layers: vec![input("bytes"), input("bytes")],
        },
        ReadOperation::LookupNamespace {
            namespace_snapshot: "snapshot".into(),
            native_id: "inode".into(),
            inputs: vec![input("bytes"), input("bytes")],
        },
    ] {
        assert!(matches!(
            operation.validate(),
            Err(OperationModelError::DuplicateInput)
        ));
        let mut invalid = plan();
        invalid.operations[1].operation = operation;
        assert_rejected_plan(&invalid);
    }
}

#[test]
fn renaming_a_recovery_role_does_not_create_a_second_helper() {
    let mut repeated = input("bytes");
    repeated.role = "another-role".into();
    let operation = ReadOperation::RecoverRange {
        helpers: vec![input("bytes"), repeated],
        algorithm: algorithm(),
    };
    assert!(matches!(
        operation.validate(),
        Err(OperationModelError::DuplicateRecoveryHelper)
    ));
    let mut invalid = plan();
    invalid.operations[1].operation = operation;
    assert_rejected_plan(&invalid);
}

#[test]
fn disjoint_ranges_from_one_parent_can_share_the_same_input_role() {
    let mut valid = translated_window_plan();
    valid.operations[1].operation = ReadOperation::MapExtents {
        mapping: mapping(),
        inputs: vec![
            OperationInput {
                range: ByteRange::new(1024, 2).unwrap(),
                ..input("bytes")
            },
            OperationInput {
                range: ByteRange::new(1030, 2).unwrap(),
                ..input("bytes")
            },
        ],
    };
    valid.validate().unwrap();
    assert_eq!(
        serde_json::from_str::<ReadOperationPlan>(&serde_json::to_string(&valid).unwrap()).unwrap(),
        valid
    );
}

#[test]
fn one_parent_range_can_serve_different_explicit_nonrecovery_roles() {
    let mut valid = plan();
    let mut dictionary = input("bytes");
    dictionary.role = "dictionary".into();
    valid.operations[1].operation = ReadOperation::DecodeContent {
        inputs: vec![input("bytes"), dictionary],
        codec: algorithm(),
        transform_identity: "fixed-transform".into(),
    };
    valid.validate().unwrap();
    assert_eq!(
        serde_json::from_str::<ReadOperationPlan>(&serde_json::to_string(&valid).unwrap()).unwrap(),
        valid
    );
}

#[test]
fn distinct_helpers_can_share_a_native_role_without_invented_role_names() {
    let mut valid = plan();
    let mut second = valid.operations[0].clone();
    second.id = "second".into();
    if let ReadOperation::ReadBackingRange { backing, .. } = &mut second.operation {
        backing.instance_id = "second-instance".into();
    }
    valid.operations.push(second);
    valid.operations[1].operation = ReadOperation::RecoverRange {
        helpers: vec![input("bytes"), input("second")],
        algorithm: algorithm(),
    };
    valid.validate().unwrap();
    assert_eq!(
        serde_json::from_str::<ReadOperationPlan>(&serde_json::to_string(&valid).unwrap()).unwrap(),
        valid
    );
}

#[test]
fn overlay_layers_preserve_order_instead_of_becoming_an_input_set() {
    let operation = ReadOperation::ApplyOverlay {
        ordered_layers: vec![input("base"), input("delta")],
        algorithm: algorithm(),
    };
    let mut reversed = operation.clone();
    if let ReadOperation::ApplyOverlay { ordered_layers, .. } = &mut reversed {
        ordered_layers.reverse();
    }
    operation.validate().unwrap();
    reversed.validate().unwrap();
    assert_ne!(
        serde_json::to_value(operation).unwrap(),
        serde_json::to_value(reversed).unwrap()
    );
}

#[test]
fn work_deserialization_actively_rejects_empty_identity_fields_and_input_sets() {
    let original = work();
    original.validate().unwrap();
    for field in [
        "operationId",
        "adapterId",
        "adapterVersion",
        "goalId",
        "policyVersion",
    ] {
        for invalid in ["", " "] {
            let mut serialized = serde_json::to_value(&original).unwrap();
            serialized[field] = serde_json::json!(invalid);
            assert!(serde_json::from_value::<WorkIdentity>(serialized).is_err());
        }
    }
    for references in [
        serde_json::json!([]),
        serde_json::json!(["a", ""]),
        serde_json::json!(["a", " "]),
    ] {
        let mut serialized = serde_json::to_value(&original).unwrap();
        serialized["inputCapabilityVersions"] = references;
        assert!(serde_json::from_value::<WorkIdentity>(serialized).is_err());
    }
}

#[test]
fn work_deserialization_actively_rejects_invalid_digests_and_ordering() {
    for field in [
        "selectorSha256",
        "versionWitnessSha256",
        "scopeWitnessSha256",
    ] {
        let mut serialized = serde_json::to_value(work()).unwrap();
        serialized[field] = serde_json::json!("A".repeat(64));
        assert!(serde_json::from_value::<WorkIdentity>(serialized).is_err());
    }
    for ordering in [
        serde_json::json!({"kind": "unordered", "domain": "unexpected"}),
        serde_json::json!({"kind": "native_sequence", "domain": "", "boundarySha256": "a".repeat(64)}),
        serde_json::json!({"kind": "native_sequence", "domain": "journal", "boundarySha256": ""}),
        serde_json::json!({"kind": "namespace_key", "snapshot": "", "descending": false}),
        serde_json::json!({"kind": "namespace_key", "snapshot": "fixed"}),
    ] {
        let mut serialized = serde_json::to_value(work()).unwrap();
        serialized["ordering"] = ordering;
        assert!(serde_json::from_value::<WorkIdentity>(serialized).is_err());
    }
}

#[test]
fn every_work_ordering_round_trips_and_changes_work_identity() {
    let mut identities = BTreeSet::new();
    for ordering in [
        OrderingRequirement::Unordered,
        OrderingRequirement::NativeSequence {
            domain: "journal".into(),
            boundary_sha256: "d".repeat(64),
        },
        OrderingRequirement::NamespaceKey {
            snapshot: "fixed".into(),
            descending: false,
        },
        OrderingRequirement::NamespaceKey {
            snapshot: "fixed".into(),
            descending: true,
        },
    ] {
        let work = WorkIdentity { ordering, ..work() };
        work.validate().unwrap();
        assert!(identities.insert(work.key().unwrap()));
        assert_eq!(
            serde_json::from_str::<WorkIdentity>(&serde_json::to_string(&work).unwrap()).unwrap(),
            work
        );
    }
}

fn translated_window_plan() -> ReadOperationPlan {
    ReadOperationPlan {
        view_id: "view".into(),
        target_id: "target".into(),
        root_operation_id: "window".into(),
        operations: vec![
            ReadOperationNode {
                id: "bytes".into(),
                output_space: AddressSpace::PhysicalByte,
                output_range: ByteRange::new(1024, 8).unwrap(),
                operation: ReadOperation::ReadBackingRange {
                    backing: backing(),
                    range: ByteRange::new(1024, 8).unwrap(),
                },
            },
            ReadOperationNode {
                id: "window".into(),
                output_space: AddressSpace::File,
                output_range: ByteRange::new(0, 4).unwrap(),
                operation: ReadOperation::Window {
                    input: OperationInput {
                        range: ByteRange::new(1026, 4).unwrap(),
                        ..input("bytes")
                    },
                    output_range: ByteRange::new(0, 4).unwrap(),
                },
            },
        ],
    }
}

fn work() -> WorkIdentity {
    WorkIdentity {
        operation_id: "inspect".into(),
        adapter_id: "adapter".into(),
        adapter_version: "1".into(),
        goal_id: "goal".into(),
        input_capability_versions: BTreeSet::from(["a".into(), "b".into()]),
        selector_sha256: "a".repeat(64),
        version_witness_sha256: "b".repeat(64),
        scope_witness_sha256: "c".repeat(64),
        ordering: OrderingRequirement::Unordered,
        policy_version: "1".into(),
    }
}

fn assert_rejected_plan(plan: &ReadOperationPlan) {
    let serialized = serde_json::to_string(plan).unwrap();
    assert!(serde_json::from_str::<ReadOperationPlan>(&serialized).is_err());
}
