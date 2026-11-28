use domain::structure::{
    ByteRange, NamespaceFilter, NamespaceOrdering, NamespaceQuery, RuntimeScope, TargetEventClock,
    TargetEventOrdering, TargetEventTimeRange, TargetKeyBound, TargetKeyOrdering, TargetKeyRange,
    TargetSelector, TargetSelectorError, TargetSequenceRange, VersionBoundary,
};
use domain::FileEntryId;

#[test]
fn all_seven_target_variants_round_trip_and_have_distinct_identities() {
    let mut identities = std::collections::BTreeSet::new();
    for selector in selectors() {
        selector.validate().unwrap();
        assert_eq!(selector.view_id(), "fixed-view");
        let serialized = serde_json::to_string(&selector).unwrap();
        assert_eq!(
            serde_json::from_str::<TargetSelector>(&serialized).unwrap(),
            selector
        );
        assert!(identities.insert(selector.identity().unwrap()));
    }
    assert_eq!(identities.len(), 7);
}

#[test]
fn targets_reject_unknown_fields_variants_and_missing_fixed_view_identity() {
    for selector in selectors() {
        let mut value = serde_json::to_value(&selector).unwrap();
        value["unknownField"] = serde_json::json!(true);
        assert!(serde_json::from_value::<TargetSelector>(value).is_err());
        let mut value = serde_json::to_value(&selector).unwrap();
        value["viewId"] = serde_json::json!("");
        assert!(serde_json::from_value::<TargetSelector>(value).is_err());
        let mut value = serde_json::to_value(&selector).unwrap();
        value.as_object_mut().unwrap().remove("viewId");
        assert!(serde_json::from_value::<TargetSelector>(value).is_err());
    }
    assert!(serde_json::from_value::<TargetSelector>(serde_json::json!({
        "kind": "process", "viewId": "fixed-view"
    }))
    .is_err());
}

#[test]
fn block_target_preserves_exact_u64_bytes_and_refuses_range_overflow() {
    let original = TargetSelector::Block {
        view_id: "fixed-view".into(),
        range: ByteRange::new(u64::MAX - 4096, 4096).unwrap(),
    };
    let serialized = serde_json::to_string(&original).unwrap();
    assert!(serialized.contains("18446744073709547519"));
    assert_eq!(
        serde_json::from_str::<TargetSelector>(&serialized).unwrap(),
        original
    );
    let mut value = serde_json::to_value(original).unwrap();
    value["range"]["length"] = serde_json::json!(4097);
    assert!(serde_json::from_value::<TargetSelector>(value).is_err());
}

#[test]
fn target_identity_preserves_adjacent_u64_positions_above_float_precision() {
    let first = TargetSelector::Block {
        view_id: "fixed-view".into(),
        range: ByteRange::new(9_007_199_254_740_992, 1).unwrap(),
    };
    let second = TargetSelector::Block {
        view_id: "fixed-view".into(),
        range: ByteRange::new(9_007_199_254_740_993, 1).unwrap(),
    };
    assert_ne!(first.identity().unwrap(), second.identity().unwrap());
}

#[test]
fn file_identity_binds_snapshot_entry_stream_and_requested_range() {
    let original = selectors().remove(1);
    let identity = original.identity().unwrap();
    for (field, value) in [
        ("namespaceSnapshot", serde_json::json!("snapshot-2")),
        ("fileEntryId", serde_json::json!("other-file-entry")),
        ("stream", serde_json::json!("alternate-stream")),
        ("range", serde_json::json!({"start": 0, "length": 1})),
    ] {
        let mut changed = serde_json::to_value(&original).unwrap();
        changed[field] = value;
        let selector: TargetSelector = serde_json::from_value(changed).unwrap();
        assert_ne!(identity, selector.identity().unwrap());
    }
    let mut value = serde_json::to_value(original).unwrap();
    value["stream"] = serde_json::json!("");
    assert!(serde_json::from_value::<TargetSelector>(value).is_err());
}

#[test]
fn object_selection_preserves_default_namespace_and_native_version() {
    let original = TargetSelector::Object {
        view_id: "rados-view".into(),
        native_namespace: String::new(),
        object_id: "rbd_data.image-id.0000000000000000".into(),
        object_version: "epoch=17,version=18446744073709551615".into(),
        range: None,
    };
    original.validate().unwrap();
    let serialized = serde_json::to_string(&original).unwrap();
    assert_eq!(
        serde_json::from_str::<TargetSelector>(&serialized).unwrap(),
        original
    );
    let mut value = serde_json::to_value(&original).unwrap();
    value["objectVersion"] = serde_json::json!("epoch=18,version=18446744073709551615");
    let changed: TargetSelector = serde_json::from_value(value).unwrap();
    assert_ne!(original.identity().unwrap(), changed.identity().unwrap());
}

#[test]
fn key_selection_preserves_binary_keys_empty_keys_and_native_comparators() {
    let ranges = [
        TargetKeyRange::All,
        TargetKeyRange::Exact { key: vec![] },
        TargetKeyRange::Exact {
            key: vec![0, 255, 128, 1],
        },
        TargetKeyRange::Interval {
            lower: Some(TargetKeyBound {
                key: vec![255],
                inclusive: true,
            }),
            upper: Some(TargetKeyBound {
                key: vec![0],
                inclusive: false,
            }),
            ordering: TargetKeyOrdering::Native {
                comparator_id: "rocksdb.ReverseBytewiseComparator".into(),
            },
        },
    ];
    for range in ranges {
        let selector = TargetSelector::Key {
            view_id: "kv-view".into(),
            native_namespace: String::new(),
            range,
            boundary: boundary("rocksdb", "sequence", "18446744073709551615"),
        };
        selector.validate().unwrap();
        let serialized = serde_json::to_string(&selector).unwrap();
        assert_eq!(
            serde_json::from_str::<TargetSelector>(&serialized).unwrap(),
            selector
        );
    }
}

#[test]
fn bytewise_key_intervals_validate_native_endpoint_inclusion() {
    let range = TargetKeyRange::Interval {
        lower: Some(TargetKeyBound {
            key: vec![1, 2],
            inclusive: true,
        }),
        upper: Some(TargetKeyBound {
            key: vec![1, 2],
            inclusive: true,
        }),
        ordering: TargetKeyOrdering::Bytewise,
    };
    range.validate().unwrap();
    let mut value = serde_json::to_value(&range).unwrap();
    value["upper"]["inclusive"] = serde_json::json!(false);
    assert!(serde_json::from_value::<TargetKeyRange>(value).is_err());
    let mut value = serde_json::to_value(range).unwrap();
    value["lower"]["key"] = serde_json::json!([2]);
    assert!(serde_json::from_value::<TargetKeyRange>(value).is_err());
    assert!(TargetKeyRange::Interval {
        lower: None,
        upper: None,
        ordering: TargetKeyOrdering::Bytewise,
    }
    .validate()
    .is_err());
}

#[test]
fn namespace_query_sets_have_stable_identity_after_reordering_and_deduplication() {
    let original = selectors().remove(4);
    let mut value = serde_json::to_value(&original).unwrap();
    value["query"]["filters"] = serde_json::json!([
        {"kind": "path_prefix", "value": "/etc"},
        {"kind": "entry_kind", "nativeKind": "regular"},
        {"kind": "path_prefix", "value": "/etc"}
    ]);
    value["query"]["fields"] = serde_json::json!(["size", "name", "size"]);
    let changed: TargetSelector = serde_json::from_value(value).unwrap();
    assert_eq!(original, changed);
    assert_eq!(original.identity().unwrap(), changed.identity().unwrap());
}

#[test]
fn namespace_query_requires_explicit_projection_nonzero_page_and_valid_cursor() {
    let original = selectors().remove(4);
    for (field, value) in [
        ("fields", serde_json::json!([])),
        ("pageSize", serde_json::json!(0)),
        ("cursor", serde_json::json!("")),
        (
            "ordering",
            serde_json::json!({"kind": "native", "orderingId": ""}),
        ),
    ] {
        let mut changed = serde_json::to_value(&original).unwrap();
        changed["query"][field] = value;
        assert!(serde_json::from_value::<TargetSelector>(changed).is_err());
    }
}

#[test]
fn event_target_keeps_inclusive_native_u64_max_without_overflow() {
    let selector = selectors().remove(5);
    let serialized = serde_json::to_string(&selector).unwrap();
    assert!(serialized.contains("18446744073709551615"));
    assert_eq!(
        serde_json::from_str::<TargetSelector>(&serialized).unwrap(),
        selector
    );
    let mut changed = serde_json::to_value(&selector).unwrap();
    changed["range"] = serde_json::json!({"first": 19, "lastInclusive": 18});
    assert!(serde_json::from_value::<TargetSelector>(changed).is_err());
    let mut changed = serde_json::to_value(selector).unwrap();
    changed["time"]["endUnixMs"] = serde_json::json!(-1);
    assert!(serde_json::from_value::<TargetSelector>(changed).is_err());
}

#[test]
fn event_native_order_and_display_time_order_have_distinct_identity() {
    let original = selectors().remove(5);
    let mut changed = serde_json::to_value(&original).unwrap();
    changed["ordering"] = serde_json::json!("event_time");
    let changed: TargetSelector = serde_json::from_value(changed).unwrap();
    assert_ne!(original.identity().unwrap(), changed.identity().unwrap());
    for ordering in [
        TargetEventOrdering::NativeSequence,
        TargetEventOrdering::EventTime,
        TargetEventOrdering::ObservationTime,
        TargetEventOrdering::ReceptionTime,
    ] {
        assert_eq!(
            serde_json::from_value::<TargetEventOrdering>(serde_json::to_value(ordering).unwrap())
                .unwrap(),
            ordering
        );
    }
}

#[test]
fn runtime_targets_preserve_snapshot_process_and_region_identity() {
    let scopes = [
        RuntimeScope::Snapshot,
        RuntimeScope::Process {
            process_id: u64::MAX,
        },
        RuntimeScope::Region {
            process_id: Some(0),
            region_id: "captured-region-a".into(),
            range: Some(ByteRange::new(u64::MAX - 1024, 1024).unwrap()),
        },
        RuntimeScope::Region {
            process_id: None,
            region_id: "physical-region-a".into(),
            range: None,
        },
    ];
    let mut identities = std::collections::BTreeSet::new();
    for scope in scopes {
        let selector = TargetSelector::Runtime {
            view_id: "runtime-view".into(),
            snapshot_id: "captured-snapshot-a".into(),
            state_kind: "memory-regions".into(),
            scope,
        };
        selector.validate().unwrap();
        let encoded = serde_json::to_string(&selector).unwrap();
        assert_eq!(
            serde_json::from_str::<TargetSelector>(&encoded).unwrap(),
            selector
        );
        assert!(identities.insert(selector.identity().unwrap()));
    }
}

#[test]
fn runtime_scope_rejects_empty_regions_and_forged_overflow() {
    let mut value = serde_json::to_value(selectors().remove(6)).unwrap();
    value["scope"] = serde_json::json!({
        "kind": "region", "regionId": "", "range": null
    });
    assert!(serde_json::from_value::<TargetSelector>(value.clone()).is_err());
    value["scope"]["regionId"] = serde_json::json!("captured-region-a");
    value["scope"]["range"] = serde_json::json!({"start": u64::MAX, "length": 1});
    assert!(serde_json::from_value::<TargetSelector>(value).is_err());
}

#[test]
fn nested_unit_variants_reject_fields_instead_of_discarding_them() {
    assert!(serde_json::from_value::<TargetKeyOrdering>(
        serde_json::json!({"kind": "bytewise", "comparatorId": "unexpected"})
    )
    .is_err());
    assert!(serde_json::from_value::<TargetKeyRange>(
        serde_json::json!({"kind": "all", "key": [1]})
    )
    .is_err());
    assert!(serde_json::from_value::<NamespaceOrdering>(
        serde_json::json!({"kind": "name_ascending", "orderingId": "unexpected"})
    )
    .is_err());
    assert!(serde_json::from_value::<RuntimeScope>(
        serde_json::json!({"kind": "snapshot", "processId": 1})
    )
    .is_err());
}

#[test]
fn target_validation_reports_typed_errors_for_invalid_constructed_values() {
    let selector = TargetSelector::Namespace {
        view_id: "fixed-view".into(),
        namespace_snapshot: "fixed-snapshot".into(),
        root_id: "/".into(),
        query: NamespaceQuery {
            filters: Default::default(),
            fields: ["name".into()].into_iter().collect(),
            ordering: NamespaceOrdering::NameAscending,
            page_size: 0,
            cursor: None,
        },
    };
    assert!(matches!(
        selector.validate(),
        Err(TargetSelectorError::EmptyPage)
    ));
    assert!(selector.identity().is_err());
}

fn selectors() -> Vec<TargetSelector> {
    vec![
        TargetSelector::Block {
            view_id: "fixed-view".into(),
            range: ByteRange::new(512, 4096).unwrap(),
        },
        TargetSelector::File {
            view_id: "fixed-view".into(),
            namespace_snapshot: "fixed-directory-snapshot".into(),
            file_entry_id: FileEntryId("source-file-entry".into()),
            stream: None,
            range: None,
        },
        TargetSelector::Object {
            view_id: "fixed-view".into(),
            native_namespace: "pool-7/default".into(),
            object_id: "rbd_data.image-id.0000000000000000".into(),
            object_version: "epoch=17,version=8".into(),
            range: Some(ByteRange::new(0, 1024).unwrap()),
        },
        TargetSelector::Key {
            view_id: "fixed-view".into(),
            native_namespace: "etcd/key".into(),
            range: TargetKeyRange::Exact {
                key: b"/registry/pods/default/pod-a".to_vec(),
            },
            boundary: boundary("etcd", "applied_revision", "18446744073709551615"),
        },
        TargetSelector::Namespace {
            view_id: "fixed-view".into(),
            namespace_snapshot: "fixed-directory-snapshot".into(),
            root_id: "/".into(),
            query: NamespaceQuery {
                filters: [
                    NamespaceFilter::PathPrefix {
                        value: "/etc".into(),
                    },
                    NamespaceFilter::EntryKind {
                        native_kind: "regular".into(),
                    },
                ]
                .into_iter()
                .collect(),
                fields: ["name".into(), "size".into()].into_iter().collect(),
                ordering: NamespaceOrdering::NameAscending,
                page_size: 512,
                cursor: None,
            },
        },
        TargetSelector::Event {
            view_id: "fixed-view".into(),
            sequence_domain: "mds-rank-0-journal".into(),
            boundary: boundary("cephfs", "journal_offset", "18446744073709551615"),
            range: TargetSequenceRange {
                first: Some(u64::MAX),
                last_inclusive: Some(u64::MAX),
            },
            time: Some(TargetEventTimeRange {
                clock: TargetEventClock::Observation,
                start_unix_ms: -1,
                end_unix_ms: 1_000,
            }),
            ordering: TargetEventOrdering::NativeSequence,
        },
        TargetSelector::Runtime {
            view_id: "fixed-view".into(),
            snapshot_id: "captured-snapshot-a".into(),
            state_kind: "processes".into(),
            scope: RuntimeScope::Snapshot,
        },
    ]
}

fn boundary(adapter: &str, kind: &str, value: &str) -> VersionBoundary {
    VersionBoundary::new(adapter.into(), kind.into(), value.into(), "a".repeat(64)).unwrap()
}
