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
    persistence_sqlite::source_fjall::configure_source_fjall(&connection, &path).unwrap();
    persistence_sqlite::source_fjall::open_store_for_write(&connection)
        .unwrap()
        .expect("initialize Fjall source generation");
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
        .insert_batch(std::slice::from_ref(&entry))
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

#[test]
fn native_queries_preserve_unknown_encryption_partition_and_read_only_boundary() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.db");
    let conn = source_connection(root.path());
    let store = persistence_sqlite::source_fjall::open_store(&conn)
        .unwrap()
        .unwrap();
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let record = persistence_fjall::FileEntryRecord {
        id: "file".into(),
        parent_id: None,
        data_source_id: "source-1".into(),
        path: "evidence.txt".into(),
        name: "evidence.txt".into(),
        entry_type: persistence_fjall::CatalogEntryType::File,
        size: Some(7),
        ext: Some("txt".into()),
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        encrypted: None,
        created: None,
        modified: None,
        accessed: None,
        changed: None,
        hash_sha256: None,
        partition_index: Some(9),
        unix_mode: None,
    };
    catalog
        .upsert(&record, persistence_fjall::PersistMode::SyncAll)
        .unwrap();
    let id = FileEntryId("file".into());
    assert_eq!(
        FileRepo::new(&conn).find_encryption_status(&id).unwrap(),
        Some(domain::FileEncryptionStatus::Unknown)
    );
    assert!(
        FileRepo::new(&conn)
            .find_by_id(&id)
            .unwrap()
            .unwrap()
            .encrypted
    );
    FileRepo::new(&conn)
        .update_hash_sha256(&id, &"a".repeat(64))
        .unwrap();
    assert_eq!(
        FileRepo::new(&conn)
            .find_partition_index_by_id(&id)
            .unwrap(),
        Some(9)
    );
    assert_eq!(
        FileRepo::new(&conn).find_encryption_status(&id).unwrap(),
        Some(domain::FileEncryptionStatus::Unknown)
    );
    let read_only = persistence_sqlite::open_existing_source_read_only(&path).unwrap();
    assert!(FileRepo::new(&read_only)
        .update_hash_sha256(&id, &"b".repeat(64))
        .is_err());
    assert!(ArtifactRepo::new(&read_only)
        .delete_analysis_outputs_in_transaction("file", "parser")
        .is_err());
    assert_eq!(
        FileRepo::new(&conn)
            .find_by_id(&id)
            .unwrap()
            .unwrap()
            .hash_sha256,
        Some("a".repeat(64))
    );
    let foreign = persistence_fjall::FileEntryRecord {
        data_source_id: "foreign".into(),
        ..record
    };
    catalog
        .upsert(&foreign, persistence_fjall::PersistMode::SyncAll)
        .unwrap();
    assert_eq!(
        FileRepo::new(&conn)
            .find_by_id(&id)
            .unwrap()
            .unwrap()
            .data_source_id
            .0,
        "source-1"
    );
    assert!(persistence_sqlite::source_fjall::configure_source_identity(&conn, "foreign").is_err());
}

#[test]
fn staging_merge_folds_roots_and_replays_multiple_partitions_without_duplicates() {
    use persistence_sqlite::repositories::staging_repo::StagingRepo;
    let root = tempfile::tempdir().unwrap();
    let main = source_connection(root.path());
    let mut stages = Vec::new();
    for partition in [1, 2] {
        let stage = persistence_sqlite::connection::open_staging(
            &root.path().join(format!("stage-{partition}.db")),
        )
        .unwrap();
        for (id, parent, path, name, kind) in [
            (
                format!("raw-{partition}"),
                None,
                format!("[P{partition}]"),
                "/",
                "directory",
            ),
            (
                format!("dir-{partition}"),
                Some(format!("raw-{partition}")),
                format!("[P{partition}]/sub"),
                "sub",
                "directory",
            ),
            (
                format!("file-{partition}"),
                Some(format!("dir-{partition}")),
                format!("[P{partition}]/sub/note.txt"),
                "note.txt",
                "file",
            ),
        ] {
            stage.execute("INSERT INTO file_entries(id,parent_id,data_source_id,path,name,entry_type,partition_index,encrypted) VALUES (?1,?2,'source-1',?3,?4,?5,?6,NULL)",rusqlite::params![id,parent,path,name,kind,partition]).unwrap();
        }
        stages.push((partition, stage));
    }
    for _ in 0..2 {
        let tx = main.unchecked_transaction().unwrap();
        for (partition, stage) in &stages {
            assert_eq!(
                StagingRepo::merge_enum_staging_to_main_in_transaction(
                    &tx,
                    stage,
                    "source-1",
                    *partition,
                    "Partition"
                )
                .unwrap(),
                2
            );
        }
        tx.commit().unwrap();
    }
    let store = persistence_sqlite::source_fjall::open_store(&main)
        .unwrap()
        .unwrap();
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let page = catalog
        .list_all(&snapshot, "source-1", None, ScanDirection::Forward, 20)
        .unwrap();
    assert_eq!(page.records.len(), 6);
    assert_eq!(source_fingerprint_table_count(&main), 0);
    assert!(!page
        .records
        .iter()
        .any(|record| record.id.starts_with("raw-")));
    for partition in [1, 2] {
        let file = catalog
            .get(&snapshot, "source-1", &format!("file-{partition}"))
            .unwrap()
            .unwrap();
        assert_eq!(file.partition_index, Some(partition));
        assert_eq!(file.path, format!("[P{partition}]/sub/note.txt"));
        assert_eq!(file.encrypted, None);
        let directory = catalog
            .get(&snapshot, "source-1", &format!("dir-{partition}"))
            .unwrap()
            .unwrap();
        assert_eq!(
            directory.parent_id,
            Some(format!("partition-root:source-1:{partition}"))
        );
    }
    stages[0]
        .1
        .execute(
            "UPDATE file_entries SET data_source_id='foreign' WHERE id='file-1'",
            [],
        )
        .unwrap();
    assert!(StagingRepo::merge_enum_staging_to_main(
        &main,
        &stages[0].1,
        "source-1",
        1,
        "Partition"
    )
    .is_err());
    assert_eq!(
        catalog
            .list_all(
                &catalog.read_snapshot(),
                "source-1",
                None,
                ScanDirection::Forward,
                20
            )
            .unwrap()
            .records,
        page.records
    );
}

#[test]
fn analysis_workers_merge_to_fjall_without_attached_source_detail_tables() {
    use persistence_sqlite::repositories::staging_repo::StagingRepo;
    let root = tempfile::tempdir().unwrap();
    let main = source_connection(root.path());
    let mut workers = Vec::new();
    for worker in [0, 1] {
        let stage =
            StagingRepo::open_analysis_staging_conn(root.path(), "source-1", worker).unwrap();
        stage.execute("INSERT INTO artifact_rows(id,file_id,artifact_type,extractor_id,display_name,summary,data_json,created_at) VALUES (?1,?2,'Prefetch','prefetch','Program executed','observed','{\"runCount\":2}','2026-11-07T00:00:00Z')",rusqlite::params![format!("artifact-{worker}"),format!("file-{worker}")]).unwrap();
        stage.execute("INSERT INTO timeline_rows(id,file_id,event_type,timestamp,title,description,parser_id) VALUES (?1,?2,'FILE_EXECUTED','2026-11-07T00:00:00Z','Program executed','observed','prefetch')",rusqlite::params![format!("event-{worker}"),format!("file-{worker}")]).unwrap();
        workers.push(stage);
    }
    for _ in 0..2 {
        let tx = main.unchecked_transaction().unwrap();
        for (worker, stage) in workers.iter().enumerate() {
            assert_eq!(
                StagingRepo::merge_analysis_staging_to_main_in_transaction_with_alias(
                    &tx,
                    stage,
                    "case-1",
                    "source-1",
                    &format!("worker_{worker}")
                )
                .unwrap(),
                (1, 1)
            );
        }
        tx.commit().unwrap();
    }
    assert_eq!(ArtifactRepo::new(&main).count().unwrap(), 2);
    assert_eq!(TimelineRepo::new(&main).count().unwrap(), 2);
    assert_eq!(
        ArtifactRepo::new(&main)
            .find_by_id("artifact-1")
            .unwrap()
            .unwrap()
            .attrs["runCount"],
        2
    );
    assert_eq!(source_fingerprint_table_count(&main), 0);
    let attached: u64 = main
        .query_row("SELECT COUNT(*) FROM pragma_database_list", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(attached, 1);
    let details: u64 = main.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('artifacts','timeline_events','file_entries')",[],|row| row.get(0)).unwrap();
    assert_eq!(details, 0);
}

fn source_fingerprint_table_count(connection: &Connection) -> u64 {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'forensic_fingerprints'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}
