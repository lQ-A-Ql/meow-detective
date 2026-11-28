use std::collections::BTreeSet;

use domain::structure::{
    AcquisitionId, ByteRange, ReadPriority, SourceCaptureRef, SourceRevisionRef, StructureGoal,
    StructureRequest, StructureRequestError, StructureResourcePolicy, TargetSelector,
    VersionSelection,
};

fn capture(source: &str, acquisition: &str) -> SourceCaptureRef {
    SourceCaptureRef::new(
        SourceRevisionRef {
            case_id: "case".into(),
            source_id: source.into(),
            revision: u64::MAX,
        },
        AcquisitionId::new(acquisition).unwrap(),
    )
    .unwrap()
}

fn request() -> StructureRequest {
    StructureRequest {
        case_id: "case".into(),
        source_scope: vec![capture("source-a", "capture-a")],
        targets: vec![TargetSelector::Block {
            view_id: "view-a".into(),
            range: ByteRange::new(0, 4096).unwrap(),
        }],
        version_selection: VersionSelection::Current,
        goals: BTreeSet::from([StructureGoal::RangeRead]),
        analysis_profiles: BTreeSet::new(),
        resource_policy: StructureResourcePolicy {
            bytes: 64 * 1024 * 1024,
            readers: 2,
            handles: 4,
            cpu: 2,
            io_domains: 2,
            priority: ReadPriority::Normal,
            deadline_unix_ms: None,
            active_work_items: 32,
            page_rows: 512,
            page_bytes: 1024 * 1024,
        },
        adapter_hints: BTreeSet::new(),
    }
}

#[test]
fn request_round_trip_preserves_capture_revision_and_typed_goals() {
    let request = request();
    let encoded = serde_json::to_string(&request).unwrap();
    assert!(encoded.contains("18446744073709551615"));
    assert!(!encoded.contains("deadlineUnixMs"));
    let restored: StructureRequest = serde_json::from_str(&encoded).unwrap();
    assert_eq!(request, restored);
    assert_eq!(request.identity().unwrap(), restored.identity().unwrap());
}

#[test]
fn request_set_order_and_duplicate_members_do_not_change_identity() {
    let mut original = request();
    original.source_scope.push(capture("source-b", "capture-b"));
    original.targets.push(TargetSelector::Block {
        view_id: "view-b".into(),
        range: ByteRange::new(4096, 512).unwrap(),
    });
    let mut reordered = original.clone();
    reordered.source_scope.reverse();
    reordered
        .source_scope
        .push(original.source_scope[0].clone());
    reordered.targets.reverse();
    reordered.targets.push(original.targets[0].clone());
    assert_eq!(original.identity().unwrap(), reordered.identity().unwrap());
    reordered.source_scope[0].acquisition_id = AcquisitionId::new("new-capture").unwrap();
    assert_ne!(original.identity().unwrap(), reordered.identity().unwrap());
}

#[test]
fn cross_case_sources_and_invalid_resource_policies_are_rejected_on_decode() {
    let original = serde_json::to_value(request()).unwrap();
    let mut changed = original.clone();
    changed["sourceScope"][0]["source"]["caseId"] = "another-case".into();
    assert!(serde_json::from_value::<StructureRequest>(changed).is_err());
    for (field, invalid) in [
        ("bytes", 0),
        ("readers", 0),
        ("handles", 0),
        ("cpu", 0),
        ("ioDomains", 0),
        ("activeWorkItems", 4097),
        ("pageRows", 513),
        ("pageBytes", 64 * 1024 * 1024 + 1),
        ("deadlineUnixMs", 0),
    ] {
        let mut changed = original.clone();
        changed["resourcePolicy"][field] = serde_json::json!(invalid);
        assert!(
            serde_json::from_value::<StructureRequest>(changed).is_err(),
            "{field}"
        );
    }
}

#[test]
fn inventory_and_analysis_have_explicit_requirements() {
    let mut value = request();
    value.targets.clear();
    assert!(matches!(
        value.validate(),
        Err(StructureRequestError::Target)
    ));
    value.goals = BTreeSet::from([StructureGoal::Inventory]);
    value.validate().unwrap();
    value.goals.insert(StructureGoal::Analyze);
    assert!(matches!(
        value.validate(),
        Err(StructureRequestError::MissingAnalysisProfile)
    ));
    value.analysis_profiles.insert("linux-system".into());
    value.validate().unwrap();
    let encoded = serde_json::to_value(&value).unwrap();
    assert_eq!(value, serde_json::from_value(encoded).unwrap());
}

#[test]
fn request_identity_binds_goals_version_policy_and_hints() {
    let original = request();
    let mut alternatives = vec![original.clone(); 4];
    alternatives[0].goals.insert(StructureGoal::DeepVerify);
    alternatives[1].version_selection = VersionSelection::Exact {
        view_id: "selected-view".into(),
    };
    alternatives[2].resource_policy.priority = ReadPriority::Interactive;
    alternatives[3].adapter_hints.insert("filesystem".into());
    let identity = original.identity().unwrap();
    for alternative in alternatives {
        assert_ne!(identity, alternative.identity().unwrap());
    }
}

#[test]
fn request_rejects_unknown_fields_empty_goals_and_invalid_selectors() {
    let original = serde_json::to_value(request()).unwrap();
    let mut changed = original.clone();
    changed["compatibility"] = true.into();
    assert!(serde_json::from_value::<StructureRequest>(changed).is_err());
    let mut changed = original.clone();
    changed["goals"] = serde_json::json!([]);
    assert!(serde_json::from_value::<StructureRequest>(changed).is_err());
    let mut changed = original;
    changed["targets"][0]["viewId"] = "".into();
    assert!(serde_json::from_value::<StructureRequest>(changed).is_err());
}
