use domain::structure::{VersionBoundary, VersionSelection, VersionSelectionError};

#[test]
fn all_version_selections_round_trip_with_explicit_native_witnesses() {
    let selections = [
        VersionSelection::Current,
        VersionSelection::Exact {
            view_id: "fixed-view-version".into(),
        },
        VersionSelection::Historical {
            boundary: boundary("lvm", "vg_seqno", "18446744073709551615"),
        },
    ];
    let mut identities = std::collections::BTreeSet::new();
    for selection in selections {
        selection.validate().unwrap();
        let encoded = serde_json::to_string(&selection).unwrap();
        assert_eq!(
            serde_json::from_str::<VersionSelection>(&encoded).unwrap(),
            selection
        );
        assert!(identities.insert(selection.identity().unwrap()));
    }
}

#[test]
fn historical_boundary_preserves_composite_native_values() {
    let original = boundary(
        "etcd",
        "raft_term_index",
        "term=19,index=18446744073709551615",
    );
    let encoded = serde_json::to_string(&original).unwrap();
    assert_eq!(
        serde_json::from_str::<VersionBoundary>(&encoded).unwrap(),
        original
    );
    assert_eq!(
        serde_json::to_value(original).unwrap()["nativeValue"],
        "term=19,index=18446744073709551615"
    );
}

#[test]
fn historical_identity_binds_native_system_kind_value_and_witness() {
    let original = boundary("lvm", "vg_seqno", "9007199254740992");
    let identity = VersionSelection::Historical {
        boundary: original.clone(),
    }
    .identity()
    .unwrap();
    let mut alternatives = vec![original; 4];
    alternatives[0].adapter_id = "btrfs".into();
    alternatives[1].native_kind = "generation".into();
    alternatives[2].native_value = "9007199254740993".into();
    alternatives[3].witness_sha256 = "b".repeat(64);
    for alternative in alternatives {
        let selection = VersionSelection::Historical {
            boundary: alternative,
        };
        assert_ne!(identity, selection.identity().unwrap());
    }
}

#[test]
fn invalid_boundary_fields_and_digest_are_rejected_at_deserialization() {
    for field in ["adapterId", "nativeKind", "nativeValue", "witnessSha256"] {
        let mut value = serde_json::to_value(boundary("lvm", "vg_seqno", "7")).unwrap();
        value[field] = serde_json::json!("");
        assert!(serde_json::from_value::<VersionBoundary>(value.clone()).is_err());
        assert!(serde_json::from_value::<VersionSelection>(
            serde_json::json!({"kind": "historical", "boundary": value})
        )
        .is_err());
    }
    let mut invalid = boundary("lvm", "vg_seqno", "7");
    invalid.witness_sha256 = "A".repeat(64);
    assert!(matches!(
        invalid.validate(),
        Err(VersionSelectionError::InvalidWitness)
    ));
}

#[test]
fn selections_reject_unknown_variants_fields_and_implicit_exact_versions() {
    for value in [
        serde_json::json!({"kind": "unknown"}),
        serde_json::json!({"kind": "current", "boundary": {}}),
        serde_json::json!({"kind": "exact"}),
        serde_json::json!({"kind": "exact", "viewId": ""}),
        serde_json::json!({"kind": "exact", "view_id": "fixed"}),
        serde_json::json!({"kind": "historical"}),
        serde_json::json!({"viewId": "fixed"}),
    ] {
        assert!(serde_json::from_value::<VersionSelection>(value).is_err());
    }
    let mut value = serde_json::to_value(boundary("lvm", "vg_seqno", "7")).unwrap();
    value["unknownField"] = serde_json::json!(true);
    assert!(serde_json::from_value::<VersionBoundary>(value).is_err());
}

#[test]
fn exact_selection_identity_changes_with_the_fixed_view() {
    let first = VersionSelection::Exact {
        view_id: "view-at-native-version-7".into(),
    };
    let second = VersionSelection::Exact {
        view_id: "view-at-native-version-8".into(),
    };
    assert_ne!(first.identity().unwrap(), second.identity().unwrap());
    assert!(VersionSelection::Exact {
        view_id: " ".into()
    }
    .validate()
    .is_err());
}

fn boundary(adapter: &str, kind: &str, value: &str) -> VersionBoundary {
    VersionBoundary::new(adapter.into(), kind.into(), value.into(), "a".repeat(64)).unwrap()
}
