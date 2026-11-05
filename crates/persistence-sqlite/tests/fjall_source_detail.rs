use chrono::Utc;
use domain::{
    Artifact, ArtifactId, DataSourceId, EntryType, FileEntry, FileEntryId, TimelineEvent,
    TimelineEventId,
};
use persistence_fjall::{
    ArtifactRecordStore, CatalogRecordStore, ScanDirection, TimelineRecordStore,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, file_repo::FileRepo, timeline_repo::TimelineRepo,
};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;

fn source_connection(root: &std::path::Path) -> Connection {
    let path = root.join("source.db");
    let connection = persistence_sqlite::open_or_create_source(&path).unwrap();
    persistence_sqlite::source_fjall::configure_source_identity(&connection, "source-1").unwrap();
    connection
}

#[test]
fn source_detail_records_are_written_only_to_fjall() {
    let root = tempfile::tempdir().unwrap();
    let connection = source_connection(root.path());
    let entry = FileEntry {
        id: FileEntryId("file-1".into()),
        parent_id: None,
        data_source_id: DataSourceId("source-1".into()),
        path: "evidence.txt".into(),
        name: "evidence.txt".into(),
        entry_type: EntryType::File,
        size: Some(12),
        ext: Some("txt".into()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: Some(Utc::now()),
        accessed_at: None,
        changed_at: None,
        hash_sha256: Some("a".repeat(64)),
    };
    FileRepo::new(&connection)
        .insert_batch(&[entry.clone()])
        .unwrap();

    let artifact = Artifact {
        id: ArtifactId("artifact-1".into()),
        family: "browser.history".into(),
        title: "History".into(),
        summary: "One visit".into(),
        source_object_id: Some(entry.id.clone()),
        extractor_id: Some("test".into()),
        extractor_version: Some("1".into()),
        confidence: Some(1.0),
        source_attribution: Some("fixture".into()),
        created_at: Utc::now(),
        attrs: BTreeMap::from([("url".into(), Value::String("https://example.test".into()))]),
    };
    ArtifactRepo::new(&connection)
        .insert_batch(&[artifact], "case-1", "source-1")
        .unwrap();

    let event = TimelineEvent {
        id: TimelineEventId("event-1".into()),
        source_object_id: "file-1".into(),
        event_type: "FILE_ACCESSED".into(),
        timestamp: Utc::now(),
        title: "Accessed".into(),
        description: "fixture".into(),
        parser_id: Some("test".into()),
        parser_version: Some("1".into()),
        confidence: Some(1.0),
        source_attribution: Some("fixture".into()),
        attrs: BTreeMap::new(),
    };
    TimelineRepo::new(&connection)
        .insert_batch_with_case(&[event], "case-1")
        .unwrap();

    for table in ["file_entries", "artifacts", "timeline_events"] {
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "source SQLite must not contain {table}");
    }

    let store = persistence_fjall::open_shared_source_store(
        root.path().join("source.fjall"),
        persistence_fjall::DatabaseConfig::default(),
    )
    .unwrap();
    let snapshot = store.database().read_snapshot();
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    assert_eq!(
        catalog
            .get(&snapshot, "source-1", "file-1")
            .unwrap()
            .unwrap()
            .path,
        "evidence.txt"
    );
    let artifacts = ArtifactRecordStore::from_source_store((*store).clone())
        .list_by_source(&snapshot, "source-1", None, ScanDirection::Forward, 10)
        .unwrap();
    assert_eq!(artifacts.records.len(), 1);
    let timeline = TimelineRecordStore::from_source_store((*store).clone())
        .list_by_source(&snapshot, "source-1", None, ScanDirection::Forward, 10)
        .unwrap();
    assert_eq!(timeline.records.len(), 1);
}
