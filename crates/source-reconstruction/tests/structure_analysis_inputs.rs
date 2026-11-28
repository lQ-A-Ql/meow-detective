use std::collections::BTreeSet;

use domain::structure::{
    ByteCoverage, ByteRange, CapabilityIntegrity, CapabilityKind, CapabilityRef, CoverageContext,
    KeyBoundary, KeyCoverage, KeyOrdering, NamespaceCoverage, NamespacePage, NamespacePageEnd,
    NativeKeyRange, TargetCoverage, VersionBoundary,
};
use sha2::{Digest, Sha256};
use source_reconstruction::structure_runtime::{
    analysis_inputs::{
        match_inputs, AnalysisInputError, AnalysisInputRequirement, MinimumIntegrity,
    },
    frontier::{BindingPrerequisite, CapabilityEntry, CapabilityIndex},
};

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn integrity(level: MinimumIntegrity) -> CapabilityIntegrity {
    match level {
        MinimumIntegrity::Observed => CapabilityIntegrity::Observed,
        MinimumIntegrity::NativeVerified => CapabilityIntegrity::NativeVerified {
            verifier_id: "native-input-verifier-v1".into(),
            witness_sha256: digest(b"fixed-native-witness"),
        },
        MinimumIntegrity::ContentVerified => CapabilityIntegrity::ContentVerified {
            manifest_sha256: digest(b"fixed-content-manifest"),
            content_sha256: digest(&[0x5a; 16]),
        },
    }
}

fn context(target: &str) -> CoverageContext {
    CoverageContext {
        view_id: format!("{target}-view"),
        native_namespace: format!("{target}-namespace"),
        version: VersionBoundary {
            adapter_id: "native-input-adapter".into(),
            native_kind: "commit-boundary".into(),
            native_value: "17".into(),
            witness_sha256: digest(b"fixed-version-boundary"),
        },
    }
}

fn coverage(target: &str, kind: CapabilityKind, proven: bool) -> TargetCoverage {
    match kind {
        CapabilityKind::Namespace
        | CapabilityKind::ObjectInventory
        | CapabilityKind::ResourceRecords => TargetCoverage::Namespace(NamespaceCoverage {
            context: context(target),
            root_id: format!("{target}-root"),
            query_sha256: digest(b"select-all-native-entries"),
            ordering_id: "native-key-order".into(),
            pages: if proven {
                vec![NamespacePage {
                    input_cursor: None,
                    first_ordinal: 0,
                    object_count: 0,
                    page_sha256: digest(b"empty-native-page"),
                    verified: true,
                    end: NamespacePageEnd::Eof {
                        witness_sha256: digest(b"native-eof-witness"),
                    },
                }]
            } else {
                Vec::new()
            },
        }),
        CapabilityKind::KvInventory | CapabilityKind::KeyRange => {
            let requested = vec![NativeKeyRange {
                lower: KeyBoundary::UnboundedStart,
                upper: KeyBoundary::UnboundedEnd,
            }];
            TargetCoverage::Keys(KeyCoverage {
                context: context(target),
                ordering: KeyOrdering {
                    comparator_id: "bytewise".into(),
                    sort_key_encoding_id: "identity-bytes-v1".into(),
                    witness_sha256: digest(b"fixed-ordering-witness"),
                },
                verified: if proven {
                    requested.clone()
                } else {
                    Vec::new()
                },
                unknown: if proven {
                    Vec::new()
                } else {
                    requested.clone()
                },
                requested,
                tombstones: Vec::new(),
                unreadable: Vec::new(),
            })
        }
        _ => {
            let requested = vec![ByteRange::new(0, 16).unwrap()];
            TargetCoverage::Bytes(ByteCoverage {
                target_size: 16,
                verified: if proven {
                    requested.clone()
                } else {
                    Vec::new()
                },
                unknown: if proven {
                    Vec::new()
                } else {
                    requested.clone()
                },
                requested,
                native_holes: Vec::new(),
                unreadable: Vec::new(),
            })
        }
    }
}

fn entry(
    target: &str,
    scope: &str,
    kind: CapabilityKind,
    role: &str,
    level: MinimumIntegrity,
    revision: u64,
) -> CapabilityEntry {
    CapabilityEntry {
        capability: CapabilityRef::new(
            target.into(),
            format!("{target}-view"),
            kind,
            revision,
            integrity(level),
            coverage(target, kind, level != MinimumIntegrity::Observed),
        )
        .unwrap(),
        scope_id: scope.into(),
        native_roles: BTreeSet::from([role.into()]),
    }
}

fn requirement(
    kind: CapabilityKind,
    role: &str,
    minimum_integrity: MinimumIntegrity,
) -> AnalysisInputRequirement {
    AnalysisInputRequirement {
        kind,
        native_role: role.into(),
        minimum_integrity,
    }
}

fn prerequisite(scope: &str, kind: CapabilityKind, role: &str) -> BindingPrerequisite {
    BindingPrerequisite {
        scope_id: scope.into(),
        kind,
        native_role: role.into(),
    }
}

fn index(entries: &[CapabilityEntry]) -> CapabilityIndex {
    let mut index = CapabilityIndex::new(32).unwrap();
    for entry in entries {
        index.publish(entry.clone()).unwrap();
    }
    index
}

#[test]
fn storage_metadata_control_plane_and_guest_namespace_match_independently() {
    let osd = entry(
        "osd-kv",
        "collection-1",
        CapabilityKind::KvInventory,
        "osd-kv",
        MinimumIntegrity::NativeVerified,
        1,
    );
    let control = entry(
        "control-resources",
        "collection-1",
        CapabilityKind::ResourceRecords,
        "control-plane",
        MinimumIntegrity::NativeVerified,
        1,
    );
    let guest = entry(
        "guest-files",
        "collection-1",
        CapabilityKind::Namespace,
        "guest-files",
        MinimumIntegrity::ContentVerified,
        1,
    );
    let inputs = index(&[osd.clone(), control.clone(), guest.clone()]);
    for selected in [osd, control, guest] {
        let role = selected.native_roles.first().unwrap();
        let matched = match_inputs(
            "collection-1",
            &[requirement(
                selected.capability.kind,
                role,
                MinimumIntegrity::NativeVerified,
            )],
            &inputs,
        )
        .unwrap();
        assert_eq!(
            matched.capability_ids,
            BTreeSet::from([selected.capability.id])
        );
        assert!(matched.missing.is_empty());
    }
}

#[test]
fn metadata_match_requires_neither_guest_platform_nor_file_capability() {
    let metadata = entry(
        "osd-metadata",
        "collection-1",
        CapabilityKind::MemberMetadata,
        "osd-metadata",
        MinimumIntegrity::NativeVerified,
        1,
    );
    let inputs = index(std::slice::from_ref(&metadata));
    let matched = match_inputs(
        "collection-1",
        &[requirement(
            CapabilityKind::MemberMetadata,
            "osd-metadata",
            MinimumIntegrity::NativeVerified,
        )],
        &inputs,
    )
    .unwrap();
    assert_eq!(
        matched.capability_ids,
        BTreeSet::from([metadata.capability.id])
    );
    assert!(matched.missing.is_empty());
}

#[test]
fn unrelated_scope_kind_and_role_never_satisfy_a_requirement() {
    let inputs = index(&[
        entry(
            "other-scope",
            "collection-2",
            CapabilityKind::MemberMetadata,
            "osd",
            MinimumIntegrity::NativeVerified,
            1,
        ),
        entry(
            "other-role",
            "collection-1",
            CapabilityKind::MemberMetadata,
            "array-member",
            MinimumIntegrity::NativeVerified,
            1,
        ),
        entry(
            "other-kind",
            "collection-1",
            CapabilityKind::ContainerMetadata,
            "osd",
            MinimumIntegrity::NativeVerified,
            1,
        ),
    ]);
    let matched = match_inputs(
        "collection-1",
        &[requirement(
            CapabilityKind::MemberMetadata,
            "osd",
            MinimumIntegrity::NativeVerified,
        )],
        &inputs,
    )
    .unwrap();
    assert!(matched.capability_ids.is_empty());
    assert_eq!(
        matched.missing,
        BTreeSet::from([prerequisite(
            "collection-1",
            CapabilityKind::MemberMetadata,
            "osd",
        )])
    );
}

#[test]
fn minimum_integrity_matches_the_complete_three_by_three_matrix() {
    for actual in [
        MinimumIntegrity::Observed,
        MinimumIntegrity::NativeVerified,
        MinimumIntegrity::ContentVerified,
    ] {
        let selected = entry(
            "metadata",
            "collection-1",
            CapabilityKind::MemberMetadata,
            "member",
            actual,
            1,
        );
        let inputs = index(std::slice::from_ref(&selected));
        for minimum in [
            MinimumIntegrity::Observed,
            MinimumIntegrity::NativeVerified,
            MinimumIntegrity::ContentVerified,
        ] {
            let matched = match_inputs(
                "collection-1",
                &[requirement(
                    CapabilityKind::MemberMetadata,
                    "member",
                    minimum,
                )],
                &inputs,
            )
            .unwrap();
            let accepted = matches!(
                (minimum, actual),
                (MinimumIntegrity::Observed, _)
                    | (
                        MinimumIntegrity::NativeVerified,
                        MinimumIntegrity::NativeVerified
                    )
                    | (
                        MinimumIntegrity::NativeVerified,
                        MinimumIntegrity::ContentVerified
                    )
                    | (
                        MinimumIntegrity::ContentVerified,
                        MinimumIntegrity::ContentVerified
                    )
            );
            if accepted {
                assert_eq!(
                    matched.capability_ids,
                    BTreeSet::from([selected.capability.id.clone()])
                );
                assert!(matched.missing.is_empty());
            } else {
                assert!(matched.capability_ids.is_empty());
                assert_eq!(
                    matched.missing,
                    BTreeSet::from([prerequisite(
                        "collection-1",
                        CapabilityKind::MemberMetadata,
                        "member",
                    )])
                );
            }
        }
    }
}

#[test]
fn missing_inputs_preserve_matches_for_other_independent_requirements() {
    let metadata = entry(
        "osd-kv",
        "collection-1",
        CapabilityKind::KvInventory,
        "osd-kv",
        MinimumIntegrity::NativeVerified,
        1,
    );
    let inputs = index(std::slice::from_ref(&metadata));
    let matched = match_inputs(
        "collection-1",
        &[
            requirement(
                CapabilityKind::KvInventory,
                "osd-kv",
                MinimumIntegrity::NativeVerified,
            ),
            requirement(
                CapabilityKind::Namespace,
                "guest-files",
                MinimumIntegrity::ContentVerified,
            ),
        ],
        &inputs,
    )
    .unwrap();
    assert_eq!(
        matched.capability_ids,
        BTreeSet::from([metadata.capability.id])
    );
    assert_eq!(
        matched.missing,
        BTreeSet::from([prerequisite(
            "collection-1",
            CapabilityKind::Namespace,
            "guest-files",
        )])
    );
}

#[test]
fn equal_content_hashes_preserve_distinct_targets_and_every_immutable_revision() {
    let first = entry(
        "capture-a",
        "collection-1",
        CapabilityKind::MemberMetadata,
        "member",
        MinimumIntegrity::ContentVerified,
        1,
    );
    let second = entry(
        "capture-b",
        "collection-1",
        CapabilityKind::MemberMetadata,
        "member",
        MinimumIntegrity::ContentVerified,
        1,
    );
    let historical = entry(
        "capture-a",
        "collection-1",
        CapabilityKind::MemberMetadata,
        "member",
        MinimumIntegrity::ContentVerified,
        u64::MAX,
    );
    assert_eq!(first.capability.integrity, second.capability.integrity);
    assert_ne!(first.capability.id, second.capability.id);
    assert_ne!(first.capability.id, historical.capability.id);
    let inputs = index(&[historical.clone(), second.clone(), first.clone()]);
    let matched = match_inputs(
        "collection-1",
        &[requirement(
            CapabilityKind::MemberMetadata,
            "member",
            MinimumIntegrity::ContentVerified,
        )],
        &inputs,
    )
    .unwrap();
    assert_eq!(
        matched.capability_ids,
        BTreeSet::from([
            first.capability.id,
            second.capability.id,
            historical.capability.id,
        ])
    );
    assert!(matched.missing.is_empty());
}

#[test]
fn one_capability_satisfying_multiple_roles_is_returned_once() {
    let mut selected = entry(
        "metadata",
        "collection-1",
        CapabilityKind::MemberMetadata,
        "member",
        MinimumIntegrity::NativeVerified,
        1,
    );
    selected.native_roles.insert("metadata".into());
    let mut inputs = CapabilityIndex::with_limits(1, 2).unwrap();
    inputs.publish(selected.clone()).unwrap();
    let requirements = ["member", "metadata"]
        .into_iter()
        .map(|role| {
            requirement(
                CapabilityKind::MemberMetadata,
                role,
                MinimumIntegrity::NativeVerified,
            )
        })
        .collect::<Vec<_>>();
    let matched = match_inputs("collection-1", &requirements, &inputs).unwrap();
    assert_eq!(
        matched.capability_ids,
        BTreeSet::from([selected.capability.id])
    );
    assert!(matched.missing.is_empty());
}

#[test]
fn requirements_and_publication_order_do_not_change_the_matched_sets() {
    let first = entry(
        "metadata-a",
        "collection-1",
        CapabilityKind::MemberMetadata,
        "member",
        MinimumIntegrity::NativeVerified,
        1,
    );
    let second = entry(
        "metadata-b",
        "collection-1",
        CapabilityKind::ContainerMetadata,
        "container",
        MinimumIntegrity::ContentVerified,
        1,
    );
    let forward = index(&[first.clone(), second.clone()]);
    let reverse = index(&[second, first]);
    let mut requirements = vec![
        requirement(
            CapabilityKind::MemberMetadata,
            "member",
            MinimumIntegrity::NativeVerified,
        ),
        requirement(
            CapabilityKind::ContainerMetadata,
            "container",
            MinimumIntegrity::ContentVerified,
        ),
    ];
    let expected = match_inputs("collection-1", &requirements, &forward).unwrap();
    requirements.reverse();
    assert_eq!(
        match_inputs("collection-1", &requirements, &reverse).unwrap(),
        expected
    );
}

#[test]
fn empty_scope_roles_and_requirements_fail_before_matching() {
    let inputs = CapabilityIndex::new(1).unwrap();
    let valid = requirement(
        CapabilityKind::MemberMetadata,
        "member",
        MinimumIntegrity::NativeVerified,
    );
    for scope in ["", " \t\n"] {
        assert!(matches!(
            match_inputs(scope, std::slice::from_ref(&valid), &inputs),
            Err(AnalysisInputError::EmptyScope)
        ));
    }
    assert!(matches!(
        match_inputs("collection-1", &[], &inputs),
        Err(AnalysisInputError::EmptyRequirements)
    ));
    for role in ["", " \t\n"] {
        let invalid = requirement(
            CapabilityKind::MemberMetadata,
            role,
            MinimumIntegrity::NativeVerified,
        );
        assert!(matches!(
            match_inputs("collection-1", &[valid.clone(), invalid], &inputs),
            Err(AnalysisInputError::EmptyRole)
        ));
    }
    assert!(inputs.is_empty());
}

#[test]
fn duplicate_requirements_and_conflicting_integrity_configuration_are_rejected() {
    let inputs = CapabilityIndex::new(1).unwrap();
    let original = requirement(
        CapabilityKind::MemberMetadata,
        "member",
        MinimumIntegrity::NativeVerified,
    );
    assert!(matches!(
        match_inputs("collection-1", &[original.clone(), original.clone()], &inputs),
        Err(AnalysisInputError::DuplicateRequirement {
            kind: CapabilityKind::MemberMetadata,
            native_role,
        }) if native_role == "member"
    ));
    for level in [
        MinimumIntegrity::Observed,
        MinimumIntegrity::ContentVerified,
    ] {
        let conflicting = requirement(CapabilityKind::MemberMetadata, "member", level);
        assert!(matches!(
            match_inputs(
                "collection-1",
                &[original.clone(), conflicting.clone()],
                &inputs,
            ),
            Err(AnalysisInputError::ConflictingRequirement {
                kind: CapabilityKind::MemberMetadata,
                native_role,
            }) if native_role == "member"
        ));
        assert!(matches!(
            match_inputs("collection-1", &[conflicting, original.clone()], &inputs),
            Err(AnalysisInputError::ConflictingRequirement { .. })
        ));
    }
    assert!(inputs.is_empty());
}
