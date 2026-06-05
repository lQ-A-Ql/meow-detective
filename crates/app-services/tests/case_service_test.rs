use app_services::case_service;
use domain::{Artifact, ArtifactId, DataSource, DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, audit_repo::AuditRepo, datasource_repo::DataSourceRepo,
    file_repo::FileRepo, job_repo::JobRepo, timeline_repo::TimelineRepo,
};
use serde_json::Value;
use std::collections::BTreeMap;
use tempfile::TempDir;

#[test]
fn create_case_creates_directory_structure() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(tmp.path(), "test-case", Some("tester")).unwrap();
    assert_eq!(active.meta.name, "test-case");
    assert_eq!(active.meta.examiner.as_deref(), Some("tester"));

    let case_root = tmp.path().join("test-case");
    assert!(case_root.join("case.json").exists());
    assert!(case_root.join("app.db").exists());
    assert!(case_root.join("evidence").exists());
    assert!(case_root.join("exports").exists());
    assert!(case_root.join("reports").exists());
    assert!(case_root.join("indexes").exists());
    assert!(case_root.join("cache").exists());
    assert!(case_root.join("logs").exists());
}

#[test]
fn create_case_initializes_db() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(tmp.path(), "db-test", None).unwrap();

    let metrics = active.with_conn(|conn| {
        let version = persistence_sqlite::runner::current_version(conn)?;
        assert_eq!(
            version,
            Some(persistence_sqlite::runner::latest_version().to_string())
        );

        let repo = persistence_sqlite::repositories::case_repo::CaseRepo::new(conn);
        let found = repo.find_by_id(&active.meta.id)?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "db-test");
        Ok(())
    });
    metrics.unwrap();
}

#[test]
fn open_case_reads_metadata() {
    let tmp = TempDir::new().unwrap();
    let created = case_service::create_case(tmp.path(), "open-test", Some("examiner-1")).unwrap();
    drop(created);

    let opened = case_service::open_case(&tmp.path().join("open-test")).unwrap();
    assert_eq!(opened.case_root, tmp.path().join("open-test"));
    assert_eq!(opened.meta.examiner.as_deref(), Some("examiner-1"));
}

#[test]
fn create_duplicate_name_fails() {
    let tmp = TempDir::new().unwrap();
    case_service::create_case(tmp.path(), "dup", None).unwrap();
    let result = case_service::create_case(tmp.path(), "dup", None);
    assert!(result.is_err());
}

#[test]
fn open_nonexistent_case_fails() {
    let tmp = TempDir::new().unwrap();
    let result = case_service::open_case(&tmp.path().join("does-not-exist"));
    assert!(result.is_err());
}

#[test]
fn open_case_without_json_fails() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("bad-case");
    std::fs::create_dir_all(&dir).unwrap();
    let result = case_service::open_case(&dir);
    assert!(result.is_err());
}

#[test]
fn active_case_connection_works() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(tmp.path(), "conn-test", None).unwrap();

    let count = active
        .with_conn(|conn| {
            let n: i64 = conn.query_row("SELECT COUNT(*) FROM cases", [], |r| r.get(0))?;
            Ok(n)
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn reopen_case_shares_no_state() {
    let tmp = TempDir::new().unwrap();
    let active1 = case_service::create_case(tmp.path(), "reopen", None).unwrap();
    let case_id = active1.meta.id.clone();
    drop(active1);

    let active2 = case_service::open_case(&tmp.path().join("reopen")).unwrap();
    assert_eq!(active2.meta.id, case_id);
}

#[test]
fn delete_data_source_cascades_rows_and_writes_audit_log() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "delete-ds", Some("tester")).unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let ds_id = DataSourceId("ds-delete".to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &DataSource {
                    id: ds_id.clone(),
                    name: "delete-me".to_string(),
                    kind: domain::DataSourceKind::LogicalDirectory,
                    source_path: tmp.path().join("evidence"),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let file_id = FileEntryId("file-delete".to_string());
            FileRepo::new(conn).insert_batch(&[FileEntry {
                id: file_id.clone(),
                parent_id: None,
                data_source_id: ds_id.clone(),
                path: "note.txt".to_string(),
                name: "note.txt".to_string(),
                entry_type: EntryType::File,
                size: Some(4),
                ext: Some("txt".to_string()),
                deleted: false,
                created_at: None,
                modified_at: Some(chrono::Utc::now()),
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            }])?;

            TimelineRepo::new(conn).insert_batch_with_case(
                &[domain::TimelineEvent {
                    id: domain::TimelineEventId("tl-delete".to_string()),
                    source_object_id: file_id.0.clone(),
                    event_type: "FILE_MODIFIED".to_string(),
                    timestamp: chrono::Utc::now(),
                    title: "Modified".to_string(),
                    description: String::new(),
                    parser_id: None,
                    parser_version: None,
                    confidence: None,
                    source_attribution: None,
                    attrs: BTreeMap::new(),
                }],
                &case_id.0,
            )?;

            ArtifactRepo::new(conn).insert_batch(
                &[Artifact {
                    id: ArtifactId("artifact-delete".to_string()),
                    family: "Test".to_string(),
                    title: "Test artifact".to_string(),
                    summary: String::new(),
                    source_object_id: Some(file_id.clone()),
                    extractor_id: None,
                    extractor_version: None,
                    confidence: None,
                    source_attribution: None,
                    created_at: chrono::Utc::now(),
                    attrs: BTreeMap::<String, Value>::new(),
                }],
                &case_id.0,
                &ds_id.0,
            )?;

            let job_id = JobRepo::new(conn).create(&case_id.0, "Import")?;
            assert!(!job_id.0.is_empty());

            case_service::delete_data_source(conn, &ds_id.0).unwrap();

            for (table, column) in [
                ("data_sources", "id"),
                ("file_entries", "data_source_id"),
                ("artifacts", "data_source_id"),
            ] {
                let count: i64 = conn.query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE {column} = ?1"),
                    [ds_id.0.as_str()],
                    |row| row.get(0),
                )?;
                assert_eq!(count, 0, "{table} should not retain deleted data source");
            }

            let timeline_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM timeline_events WHERE source_object_id = ?1",
                [file_id.0.as_str()],
                |row| row.get(0),
            )?;
            assert_eq!(timeline_count, 0);

            let audit_count = AuditRepo::new(conn).count_by_action("datasource.delete")?;
            assert_eq!(audit_count, 1);

            Ok(())
        })
        .unwrap();
}
