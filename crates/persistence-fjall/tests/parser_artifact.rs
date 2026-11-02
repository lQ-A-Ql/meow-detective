use persistence_fjall::{
    ArtifactRecord, ArtifactRecordStore, DatabaseConfig, ParserRecord, ParserRecordStore,
    PersistMode, ScanDirection,
};
use serde_json::json;

fn parser(id: &str, source: &str, family: &str, kind: &str) -> ParserRecord {
    ParserRecord {
        id: id.into(),
        data_source_id: source.into(),
        family: family.into(),
        kind: kind.into(),
        status: "ready".into(),
        summary_sha256: "a".repeat(64),
        payload: json!({"id": id}),
    }
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
fn parser_records_reject_invalid_identity() {
    let root = tempfile::tempdir().unwrap();
    let store = ParserRecordStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let mut record = parser("bad", "source", "linux", "fact");
    record.family.clear();
    assert!(store.upsert(&record, PersistMode::SyncData).is_err());
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
