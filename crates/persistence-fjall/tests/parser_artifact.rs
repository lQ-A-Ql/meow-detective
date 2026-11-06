use persistence_fjall::{
    ArtifactRecord, ArtifactRecordStore, DatabaseConfig, ParserFieldQuery, ParserRecord,
    ParserRecordStore, PersistMode, ScanDirection,
};
use serde_json::json;
use sha2::{Digest, Sha256};

fn parser(id: &str, source: &str, family: &str, kind: &str) -> ParserRecord {
    let payload = json!({"id": id, "locator": format!("locator-{id}")});
    ParserRecord {
        id: id.into(),
        data_source_id: source.into(),
        family: family.into(),
        kind: kind.into(),
        status: "ready".into(),
        summary_sha256: hex::encode(Sha256::digest(payload.to_string().as_bytes())),
        payload,
    }
}

#[test]
fn artifact_revision_tracks_equal_count_replacement_and_rejects_invalid_batches() {
    let root = tempfile::tempdir().unwrap();
    let store = ArtifactRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let original = ArtifactRecord {
        id: "one".into(),
        data_source_id: "source".into(),
        artifact_type: "old".into(),
        source_object_id: Some("file".into()),
        content_sha256: None,
        attrs: json!({"value":1}),
    };
    store.upsert(&original, PersistMode::SyncAll).unwrap();
    let before = store.read_snapshot();
    let revision = store.cursor_revision(&before).unwrap();
    assert_eq!(store.count(&before, None).unwrap(), 1);
    store.upsert(&original, PersistMode::SyncAll).unwrap();
    assert_eq!(
        store.cursor_revision(&store.read_snapshot()).unwrap(),
        revision
    );
    let updated = ArtifactRecord {
        artifact_type: "new".into(),
        attrs: json!({"value":2}),
        ..original.clone()
    };
    store
        .upsert_batch(&[original.clone(), updated.clone()], PersistMode::SyncAll)
        .unwrap();
    let after = store.read_snapshot();
    assert_eq!(store.count(&after, Some("source")).unwrap(), 1);
    assert_eq!(store.cursor_revision(&after).unwrap(), revision + 1);
    assert_eq!(store.get(&before, "source", "one").unwrap(), Some(original));
    assert!(store
        .list_by_type(&after, "source", "old", None, ScanDirection::Forward, 10)
        .unwrap()
        .records
        .is_empty());
    assert_eq!(
        store
            .list_by_type(&after, "source", "new", None, ScanDirection::Forward, 10)
            .unwrap()
            .records,
        vec![updated.clone()]
    );
    let invalid = ArtifactRecord {
        id: String::new(),
        ..updated.clone()
    };
    assert!(store
        .upsert_batch(&[updated.clone(), invalid], PersistMode::SyncAll)
        .is_err());
    assert_eq!(
        store.cursor_revision(&store.read_snapshot()).unwrap(),
        revision + 1
    );
    assert_eq!(
        store
            .delete_batch(
                "source",
                &["one".into(), "one".into()],
                PersistMode::SyncAll
            )
            .unwrap(),
        1
    );
    assert_eq!(
        store.cursor_revision(&store.read_snapshot()).unwrap(),
        revision + 2
    );
}

#[test]
fn concurrent_artifact_updates_keep_one_current_type_index() {
    let root = tempfile::tempdir().unwrap();
    let store = ArtifactRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    std::thread::scope(|scope| {
        for family in ["a", "b"] {
            let writer = store.clone();
            scope.spawn(move || {
                for value in 0..50 {
                    writer
                        .upsert(
                            &ArtifactRecord {
                                id: "shared".into(),
                                data_source_id: "source".into(),
                                artifact_type: family.into(),
                                source_object_id: None,
                                content_sha256: None,
                                attrs: json!({"value":value}),
                            },
                            PersistMode::Buffer,
                        )
                        .unwrap();
                }
            });
        }
    });
    let snapshot = store.read_snapshot();
    let expected = store.get(&snapshot, "source", "shared").unwrap().unwrap();
    let indexed = ["a", "b"]
        .into_iter()
        .flat_map(|kind| {
            store
                .list_by_type(&snapshot, "source", kind, None, ScanDirection::Forward, 10)
                .unwrap()
                .records
        })
        .collect::<Vec<_>>();
    assert_eq!(indexed, vec![expected]);
}

#[test]
fn parser_field_index_is_exact_and_keyset_paginated() {
    let root = tempfile::tempdir().unwrap();
    let store = ParserRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    store
        .upsert(
            &parser("one", "source", "ceph", "ceph_fs_metadata_objects"),
            PersistMode::SyncData,
        )
        .unwrap();
    store
        .upsert(
            &parser("two", "source", "ceph", "ceph_fs_metadata_objects"),
            PersistMode::SyncData,
        )
        .unwrap();
    let snapshot = store.read_snapshot();
    let page = store
        .list_by_field(
            &snapshot,
            ParserFieldQuery {
                source_id: "source",
                kind: "ceph_fs_metadata_objects",
                field: "locator",
                value: "locator-one",
            },
            None,
            ScanDirection::Forward,
            10,
        )
        .unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].id, "one");
}

#[test]
fn parser_field_index_replaces_old_value_atomically() {
    let root = tempfile::tempdir().unwrap();
    let store = ParserRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let mut record = parser("replace", "source", "ceph", "ceph_fs_metadata_objects");
    if let serde_json::Value::Object(fields) = &mut record.payload {
        fields.insert("locator".to_owned(), "old-locator".into());
    }
    record.summary_sha256 = hex::encode(Sha256::digest(record.payload.to_string().as_bytes()));
    store.upsert(&record, PersistMode::SyncData).unwrap();
    if let serde_json::Value::Object(fields) = &mut record.payload {
        fields.insert("locator".to_owned(), "new-locator".into());
    }
    record.summary_sha256 = hex::encode(Sha256::digest(record.payload.to_string().as_bytes()));
    store.upsert(&record, PersistMode::SyncData).unwrap();
    let snapshot = store.read_snapshot();
    assert!(store
        .list_by_field(
            &snapshot,
            ParserFieldQuery {
                source_id: "source",
                kind: "ceph_fs_metadata_objects",
                field: "locator",
                value: "old-locator",
            },
            None,
            ScanDirection::Forward,
            10,
        )
        .unwrap()
        .records
        .is_empty());
    assert_eq!(
        store
            .list_by_field(
                &snapshot,
                ParserFieldQuery {
                    source_id: "source",
                    kind: "ceph_fs_metadata_objects",
                    field: "locator",
                    value: "new-locator",
                },
                None,
                ScanDirection::Forward,
                10,
            )
            .unwrap()
            .records
            .len(),
        1
    );
}

#[test]
fn parser_records_are_source_isolated_and_keyset_indexed() {
    let root = tempfile::tempdir().unwrap();
    let store = ParserRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    store
        .upsert(
            &parser("one", "source-a", "android", "package"),
            PersistMode::SyncData,
        )
        .unwrap();
    store
        .upsert(
            &parser("two", "source-b", "android", "package"),
            PersistMode::SyncData,
        )
        .unwrap();
    let snapshot = store.read_snapshot();
    let page = store
        .list_by_family(
            &snapshot,
            "source-a",
            "android",
            None,
            ScanDirection::Forward,
            10,
        )
        .unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].id, "one");
}

#[test]
fn parser_batch_upsert_writes_all_records_and_indexes() {
    let root = tempfile::tempdir().unwrap();
    let store = ParserRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let records = vec![
        parser("batch-one", "source", "ceph", "ceph_fs_metadata_objects"),
        parser("batch-two", "source", "ceph", "ceph_fs_metadata_objects"),
    ];
    store.upsert_batch(&records, PersistMode::SyncData).unwrap();
    let snapshot = store.read_snapshot();
    let page = store
        .list_by_kind(
            &snapshot,
            "source",
            "ceph_fs_metadata_objects",
            None,
            ScanDirection::Forward,
            10,
        )
        .unwrap();
    assert_eq!(page.records, records);
}

#[test]
fn parser_records_reject_invalid_identity() {
    let root = tempfile::tempdir().unwrap();
    let store = ParserRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let mut record = parser("bad", "source", "linux", "fact");
    record.family.clear();
    assert!(store.upsert(&record, PersistMode::SyncData).is_err());
}

#[test]
fn parser_records_reject_summary_digest_mismatch() {
    let root = tempfile::tempdir().unwrap();
    let store = ParserRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let mut record = parser("bad-digest", "source", "linux", "fact");
    record.summary_sha256 = "a".repeat(64);
    assert!(store.upsert(&record, PersistMode::SyncData).is_err());
}

#[test]
fn parser_family_oracle_covers_android_linux_ceph_and_nested_records() {
    let root = tempfile::tempdir().unwrap();
    let store = ParserRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    for (family, kind) in [
        ("android", "package"),
        ("linux", "evidence_facts"),
        ("ceph", "ceph_rocksdb_latest_state"),
        ("nested", "lineage"),
    ] {
        let record = parser(&format!("{family}-record"), "source", family, kind);
        store.upsert(&record, PersistMode::SyncData).unwrap();
    }
    let snapshot = store.read_snapshot();
    for (family, kind) in [
        ("android", "package"),
        ("linux", "evidence_facts"),
        ("ceph", "ceph_rocksdb_latest_state"),
        ("nested", "lineage"),
    ] {
        let page = store
            .list_by_kind(&snapshot, "source", kind, None, ScanDirection::Forward, 2)
            .unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].family, family);
        assert_eq!(page.records[0].status, "ready");
    }
}

#[test]
fn artifact_records_support_type_and_source_object_indexes() {
    let root = tempfile::tempdir().unwrap();
    let store = ArtifactRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let record = ArtifactRecord {
        id: "artifact-1".into(),
        data_source_id: "source".into(),
        artifact_type: "browser".into(),
        source_object_id: Some("file-1".into()),
        content_sha256: Some("b".repeat(64)),
        attrs: json!({"name": "x"}),
    };
    store.upsert(&record, PersistMode::SyncData).unwrap();
    let snapshot = store.read_snapshot();
    assert_eq!(
        store
            .list_by_type(
                &snapshot,
                "source",
                "browser",
                None,
                ScanDirection::Forward,
                10
            )
            .unwrap()
            .records,
        vec![record.clone()]
    );
    assert_eq!(
        store
            .list_by_source(&snapshot, "source", None, ScanDirection::Forward, 10,)
            .unwrap()
            .records,
        vec![record.clone()]
    );
    assert_eq!(
        store
            .list_by_source_object(
                &snapshot,
                "source",
                "file-1",
                None,
                ScanDirection::Forward,
                10
            )
            .unwrap()
            .records,
        vec![record]
    );
}

#[test]
fn artifact_batch_upsert_writes_all_records_and_indexes() {
    let root = tempfile::tempdir().unwrap();
    let store = ArtifactRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let records = vec![
        ArtifactRecord {
            id: "batch-artifact-1".into(),
            data_source_id: "source".into(),
            artifact_type: "ceph".into(),
            source_object_id: None,
            content_sha256: None,
            attrs: json!({"id": 1}),
        },
        ArtifactRecord {
            id: "batch-artifact-2".into(),
            data_source_id: "source".into(),
            artifact_type: "ceph".into(),
            source_object_id: None,
            content_sha256: None,
            attrs: json!({"id": 2}),
        },
    ];
    store.upsert_batch(&records, PersistMode::SyncData).unwrap();
    let snapshot = store.read_snapshot();
    assert_eq!(
        store
            .list_by_type(
                &snapshot,
                "source",
                "ceph",
                None,
                ScanDirection::Forward,
                10
            )
            .unwrap()
            .records,
        records
    );
}
