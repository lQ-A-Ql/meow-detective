use domain::structure::{
    AcquisitionId, AddressSpace, BackingVersionRef, ByteRange, LayerLocator, NativeObjectId,
    NativeVersion, NativeViewVersionRef, SourceCaptureRef, SourceRevisionRef, ViewModelError,
};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};

#[test]
fn source_capture_round_trip_preserves_the_full_unsigned_revision() {
    let capture = capture("pv-a", "acquisition-a", u64::MAX);
    capture.validate().unwrap();
    let encoded = round_trip(&capture);
    assert!(encoded.contains("18446744073709551615"));
}

#[test]
fn source_capture_rejects_missing_registration_and_empty_identities() {
    let original = capture("pv-a", "acquisition-a", 1);
    for field in ["caseId", "sourceId", "revision"] {
        let mut value = serde_json::to_value(&original).unwrap();
        value["source"][field] = if field == "revision" {
            serde_json::json!(0)
        } else {
            serde_json::json!("")
        };
        assert!(serde_json::from_value::<SourceCaptureRef>(value).is_err());
    }
    let mut value = serde_json::to_value(&original).unwrap();
    value["acquisitionId"] = serde_json::json!(" ");
    assert!(serde_json::from_value::<SourceCaptureRef>(value).is_err());
    let mut value = serde_json::to_value(original).unwrap();
    value["source"]["revision"] = serde_json::json!(1.5);
    assert!(serde_json::from_value::<SourceCaptureRef>(value).is_err());
}

#[test]
fn backing_version_round_trips_with_and_without_a_recorded_manifest() {
    for manifest in [None, Some("a".repeat(64))] {
        let backing = BackingVersionRef::new(
            capture("pv-a", "acquisition-a", 1),
            "file-instance-a".into(),
            manifest,
            "change-token-a".into(),
        )
        .unwrap();
        backing.validate().unwrap();
        round_trip(&backing);
        assert!(backing
            .identity()
            .unwrap()
            .starts_with("backing-version-v1-"));
    }
}

#[test]
fn backing_identity_binds_capture_instance_change_token_and_manifest() {
    let original = backing();
    let mut alternatives = vec![original.clone(); 5];
    alternatives[0].capture.acquisition_id = AcquisitionId::new("acquisition-b").unwrap();
    alternatives[1].capture.source.revision = 2;
    alternatives[2].instance_id = "file-instance-b".into();
    alternatives[3].change_token = "change-token-b".into();
    alternatives[4].content_manifest_sha256 = Some("b".repeat(64));
    let original_id = original.identity().unwrap();
    for alternative in alternatives {
        assert_ne!(original_id, alternative.identity().unwrap());
    }
}

#[test]
fn backing_rejects_invalid_manifest_and_empty_physical_identity() {
    for manifest in ["", "a", &"A".repeat(64), &"g".repeat(64)] {
        let mut value = serde_json::to_value(backing()).unwrap();
        value["contentManifestSha256"] = serde_json::json!(manifest);
        assert!(serde_json::from_value::<BackingVersionRef>(value).is_err());
    }
    for field in ["instanceId", "changeToken"] {
        let mut value = serde_json::to_value(backing()).unwrap();
        value[field] = serde_json::json!("");
        assert!(serde_json::from_value::<BackingVersionRef>(value).is_err());
    }
}

#[test]
fn view_constructor_normalizes_both_parent_sets() {
    let a = capture("pv-a", "acquisition-a", 1);
    let b = capture("pv-b", "acquisition-b", 1);
    let normalized = view(
        vec![a.clone(), b.clone()],
        vec!["view-a".into(), "view-b".into()],
    );
    let reordered = view(
        vec![b.clone(), a.clone(), b],
        vec!["view-b".into(), "view-a".into(), "view-a".into()],
    );
    assert_eq!(normalized, reordered);
    assert_eq!(normalized.parents.len(), 2);
    assert_eq!(normalized.parent_view_ids.len(), 2);
    normalized.validate().unwrap();
    round_trip(&normalized);
}

#[test]
fn views_preserve_distinct_acquisitions_of_identical_content() {
    let a = capture("pv-a", "acquisition-a", 1);
    let b = capture("pv-a", "acquisition-b", 1);
    let first = view(vec![a.clone()], vec![]);
    let second = view(vec![b.clone()], vec![]);
    let combined = view(vec![b, a.clone(), a], vec![]);
    assert_ne!(first.id, second.id);
    assert_ne!(first.id, combined.id);
    assert_eq!(combined.parents.len(), 2);
    round_trip(&combined);
}

#[test]
fn view_identity_distinguishes_adjacent_revisions_above_f64_precision() {
    let revisions = [9_007_199_254_740_992, 9_007_199_254_740_993, u64::MAX];
    let views = revisions.map(|revision| view(vec![capture("pv-a", "a", revision)], vec![]));
    assert_ne!(views[0].id, views[1].id);
    assert_ne!(views[1].id, views[2].id);
    for view in views {
        round_trip(&view);
    }
}

#[test]
fn different_native_commit_boundaries_produce_different_view_versions() {
    let mut first = view(vec![capture("pv-a", "a", 1)], vec![]);
    let mut second = first.clone();
    for (view, seqno) in [
        (&mut first, 9_007_199_254_740_992),
        (&mut second, 9_007_199_254_740_993),
    ] {
        let native = NativeVersion::LvmCommitted {
            vg_uuid: "vg-native-uuid".into(),
            seqno,
        };
        view.selection_sha256 = hex::encode(Sha256::digest(serde_json::to_vec(&native).unwrap()));
    }
    first = reidentify(first);
    second = reidentify(second);
    assert_ne!(first.id, second.id);
    round_trip(&first);
    round_trip(&second);
}

#[test]
fn mapping_policy_adapter_and_parent_view_changes_affect_identity() {
    let original = view(vec![capture("pv-a", "a", 1)], vec!["parent-a".into()]);
    let mut alternatives = vec![original.clone(); 4];
    alternatives[0].mapping_sha256 = "c".repeat(64);
    alternatives[1].policy_version = "policy-2".into();
    alternatives[2].adapter_version = "adapter-2".into();
    alternatives[3].parent_view_ids = vec!["parent-b".into()];
    for alternative in alternatives {
        assert_ne!(original.id, reidentify(alternative).id);
    }
}

#[test]
fn view_rejects_stale_identity_and_noncanonical_serialized_parents() {
    let original = view(
        vec![capture("pv-a", "a", 1), capture("pv-b", "b", 1)],
        vec!["parent-a".into(), "parent-b".into()],
    );
    let mut changed = original.clone();
    changed.mapping_sha256 = "c".repeat(64);
    assert!(matches!(
        changed.validate(),
        Err(ViewModelError::IdentityMismatch)
    ));
    assert!(serde_json::from_str::<NativeViewVersionRef>(
        &serde_json::to_string(&changed).unwrap()
    )
    .is_err());
    let mut changed = original.clone();
    changed.parents.reverse();
    assert!(matches!(
        changed.validate(),
        Err(ViewModelError::NonCanonicalParents)
    ));
    let mut changed = original.clone();
    changed.parent_view_ids.push("parent-b".into());
    assert!(matches!(
        changed.validate(),
        Err(ViewModelError::NonCanonicalParents)
    ));
    for changed in [changed, {
        let mut duplicate = original;
        duplicate.parents.push(duplicate.parents[1].clone());
        duplicate
    }] {
        assert!(serde_json::from_str::<NativeViewVersionRef>(
            &serde_json::to_string(&changed).unwrap()
        )
        .is_err());
    }
}

#[test]
fn view_rejects_foreign_case_missing_parents_adapter_mismatch_and_self_parent() {
    let original = view(vec![capture("pv-a", "a", 1)], vec!["parent-a".into()]);
    let mut foreign = capture("pv-b", "b", 1);
    foreign.source.case_id = "different-case".into();
    let mut changed = original.clone();
    changed.parents.push(foreign);
    assert!(matches!(
        changed.validate(),
        Err(ViewModelError::CrossCaseParents)
    ));
    let mut changed = original.clone();
    changed.parents.clear();
    changed.parent_view_ids.clear();
    assert!(matches!(
        changed.validate(),
        Err(ViewModelError::MissingParents)
    ));
    let mut changed = original.clone();
    changed.native_object_id.adapter = "another-adapter".into();
    assert!(matches!(
        changed.validate(),
        Err(ViewModelError::AdapterMismatch)
    ));
    let mut changed = original;
    changed.parent_view_ids = vec![changed.id.clone()];
    assert!(matches!(
        changed.validate(),
        Err(ViewModelError::SelfParent)
    ));
}

#[test]
fn locator_preserves_the_parent_view_and_all_native_address_spaces() {
    for address_space in [
        AddressSpace::PhysicalByte,
        AddressSpace::LogicalBlock,
        AddressSpace::File,
        AddressSpace::Object,
        AddressSpace::Key,
        AddressSpace::Namespace,
        AddressSpace::Event,
        AddressSpace::RuntimeState,
    ] {
        let mut locator = locator(address_space);
        if matches!(
            address_space,
            AddressSpace::PhysicalByte
                | AddressSpace::LogicalBlock
                | AddressSpace::File
                | AddressSpace::Object
        ) {
            locator.range = Some(ByteRange::new(u64::MAX - 128, 128).unwrap());
        }
        locator.validate().unwrap();
        round_trip(&locator);
        assert!(locator.identity().unwrap().starts_with("layer-locator-v1-"));
    }
}

#[test]
fn locator_rejects_ranges_in_native_nonbyte_spaces_and_overflowed_bytes() {
    for address_space in [
        AddressSpace::Key,
        AddressSpace::Namespace,
        AddressSpace::Event,
        AddressSpace::RuntimeState,
    ] {
        let mut locator = locator(address_space);
        locator.range = Some(ByteRange::new(0, 1).unwrap());
        assert!(matches!(
            locator.validate(),
            Err(ViewModelError::NonByteRange)
        ));
        assert!(
            serde_json::from_value::<LayerLocator>(serde_json::to_value(locator).unwrap()).is_err()
        );
    }
    let mut value = serde_json::to_value(locator(AddressSpace::File)).unwrap();
    value["range"] = serde_json::json!({"start": u64::MAX, "length": 1});
    assert!(serde_json::from_value::<LayerLocator>(value).is_err());
}

#[test]
fn all_view_records_reject_unknown_fields() {
    fn rejects_unknown<T: Serialize + DeserializeOwned>(record: &T) {
        let mut value = serde_json::to_value(record).unwrap();
        value["unknownField"] = serde_json::json!(true);
        assert!(serde_json::from_value::<T>(value).is_err());
    }
    rejects_unknown(&capture("pv-a", "a", 1));
    rejects_unknown(&backing());
    rejects_unknown(&view(vec![capture("pv-a", "a", 1)], vec![]));
    rejects_unknown(&locator(AddressSpace::File));
}

fn capture(source: &str, acquisition: &str, revision: u64) -> SourceCaptureRef {
    SourceCaptureRef::new(
        SourceRevisionRef {
            case_id: "case-a".into(),
            source_id: source.into(),
            revision,
        },
        AcquisitionId::new(acquisition).unwrap(),
    )
    .unwrap()
}

fn backing() -> BackingVersionRef {
    BackingVersionRef::new(
        capture("pv-a", "a", 1),
        "file-instance-a".into(),
        Some("a".repeat(64)),
        "change-token-a".into(),
    )
    .unwrap()
}

fn view(parents: Vec<SourceCaptureRef>, parent_views: Vec<String>) -> NativeViewVersionRef {
    NativeViewVersionRef::new(
        "lvm".into(),
        "adapter-1".into(),
        NativeObjectId::new("lvm", "logical-volume", "lv-native-uuid", "vg-native-uuid").unwrap(),
        parents,
        parent_views,
        "a".repeat(64),
        "b".repeat(64),
        "policy-1".into(),
    )
    .unwrap()
}

fn reidentify(view: NativeViewVersionRef) -> NativeViewVersionRef {
    NativeViewVersionRef::new(
        view.adapter_id,
        view.adapter_version,
        view.native_object_id,
        view.parents,
        view.parent_view_ids,
        view.selection_sha256,
        view.mapping_sha256,
        view.policy_version,
    )
    .unwrap()
}

fn locator(address_space: AddressSpace) -> LayerLocator {
    LayerLocator {
        view_id: "fixed-view".into(),
        parent_view_id: Some("parent-view".into()),
        address_space,
        native_id: "native-identity".into(),
        range: None,
        parser_version: "parser-1".into(),
    }
}

fn round_trip<T: Serialize + DeserializeOwned + std::fmt::Debug + PartialEq>(value: &T) -> String {
    let serialized = serde_json::to_string(value).unwrap();
    assert_eq!(&serde_json::from_str::<T>(&serialized).unwrap(), value);
    serialized
}
