use app_services::{case_service, source_db};
use domain::{Artifact, ArtifactId, DataSource, DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, audit_repo::AuditRepo, datasource_repo::DataSourceRepo,
    datasource_repo::DataSourceStorage, file_repo::FileRepo, job_repo::JobRepo,
    timeline_repo::TimelineRepo,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tempfile::TempDir;

static CASE_ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock_case_env() -> MutexGuard<'static, ()> {
    CASE_ENV_LOCK.lock().unwrap()
}

struct SeededSourcePaths {
    source_dir: PathBuf,
    staging_dir: PathBuf,
    evidence_marker: PathBuf,
}

fn test_data_source(id: &str, name: &str, source_path: PathBuf) -> DataSource {
    DataSource {
        id: DataSourceId(id.to_string()),
        name: name.to_string(),
        kind: domain::DataSourceKind::LogicalDirectory,
        source_path,
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    }
}

fn seed_isolated_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    source: &DataSource,
    platform: &str,
) -> persistence_sqlite::DbResult<SeededSourcePaths> {
    let storage = DataSourceStorage::source_db(&source.id.0, Some(platform), None);
    DataSourceRepo::new(case_conn).insert_with_storage(case_id, source, &storage)?;

    let source_conn = source_db::open_source_db(case_root, &source.id)?;
    DataSourceRepo::new(&source_conn).upsert_source_local_metadata(case_id, source)?;

    let file_id = FileEntryId(format!("file-{}", source.id.0));
    FileRepo::new(&source_conn).insert_batch(&[FileEntry {
        id: file_id.clone(),
        parent_id: None,
        data_source_id: source.id.clone(),
        path: "note.txt".to_string(),
        name: "note.txt".to_string(),
        entry_type: EntryType::File,
        size: Some(4),
        ext: Some("txt".to_string()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: Some(chrono::Utc::now()),
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }])?;
    ArtifactRepo::new(&source_conn).insert_batch(
        &[Artifact {
            id: ArtifactId(format!("artifact-{}", source.id.0)),
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
        &source.id.0,
    )?;
    TimelineRepo::new(&source_conn).insert_batch_with_case(
        &[domain::TimelineEvent {
            id: domain::TimelineEventId(format!("timeline-{}", source.id.0)),
            source_object_id: file_id.0,
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

    for table in ["file_entries", "artifacts", "timeline_events"] {
        let count: i64 =
            source_conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })?;
        assert_eq!(count, 1, "{table} should be stored in source.db");
    }
    source_db::checkpoint_source_db(&source_conn)?;
    drop(source_conn);

    let source_dir = source_db::source_dir(case_root, &source.id);
    let index_dir = source_db::source_index_dir(case_root, &source.id);
    std::fs::create_dir_all(&index_dir)?;
    std::fs::write(index_dir.join("index.marker"), source.id.0.as_bytes())?;

    let staging_rel_path = storage
        .staging_rel_path
        .as_deref()
        .expect("source-db storage should define a staging path");
    let staging_dir = source_db::safe_case_relative_path(case_root, staging_rel_path)?;
    std::fs::create_dir_all(&staging_dir)?;
    std::fs::write(staging_dir.join("staging.marker"), source.id.0.as_bytes())?;

    std::fs::create_dir_all(&source.source_path)?;
    let evidence_marker = source.source_path.join("original-evidence.marker");
    std::fs::write(&evidence_marker, b"immutable evidence fixture")?;

    Ok(SeededSourcePaths {
        source_dir,
        staging_dir,
        evidence_marker,
    })
}

fn assert_seeded_source_survives(paths: &SeededSourcePaths) {
    assert!(paths.source_dir.join("source.db").is_file());
    assert!(paths
        .source_dir
        .join("index")
        .join("index.marker")
        .is_file());
    assert!(paths.staging_dir.join("staging.marker").is_file());
    assert_eq!(
        std::fs::read(&paths.evidence_marker).unwrap(),
        b"immutable evidence fixture"
    );
}

fn setup_job_db() -> (rusqlite::Connection, String) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    let case_id = "test-case-1";
    conn.execute(
        "INSERT INTO cases (id, name, number, examiner) VALUES (?1, 'Test', '1', 'qa')",
        rusqlite::params![case_id],
    )
    .unwrap();
    (conn, case_id.to_string())
}

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
fn open_case_migrates_ready_source_partition_routing_before_reads() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(tmp.path(), "source-reopen", None).unwrap();
    let case_root = active.case_root.clone();
    let source = test_data_source(
        "ds-source-reopen",
        "Linux source",
        tmp.path().join("linux.E01"),
    );
    let mut storage = DataSourceStorage::source_db(&source.id.0, Some("linux"), None);
    storage.import_state = "ready".to_string();

    active
        .with_conn(|case_conn| {
            DataSourceRepo::new(case_conn).insert_with_storage(
                &active.meta.id,
                &source,
                &storage,
            )?;
            let source_conn = source_db::open_source_db(&case_root, &source.id)?;
            DataSourceRepo::new(&source_conn)
                .upsert_source_local_metadata(&active.meta.id, &source)?;
            source_conn.execute(
                "INSERT INTO file_entries
                 (id, parent_id, data_source_id, path, name, entry_type, partition_index)
                 VALUES (?1, NULL, ?2, '', 'Partition 2 (XFS) - cl/root', 'directory', NULL)",
                rusqlite::params!["root-2", source.id.0],
            )?;
            source_conn.execute(
                "INSERT INTO file_entries
                 (id, parent_id, data_source_id, path, name, entry_type, partition_index)
                 VALUES (?1, ?2, ?3, 'etc', 'etc', 'directory', NULL)",
                rusqlite::params!["etc", "root-2", source.id.0],
            )?;
            source_conn.execute(
                "INSERT INTO file_entries
                 (id, parent_id, data_source_id, path, name, entry_type, partition_index)
                 VALUES (?1, ?2, ?3, 'etc/passwd', 'passwd', 'file', NULL)",
                rusqlite::params!["passwd", "etc", source.id.0],
            )?;
            source_conn.execute(
                "DELETE FROM schema_migrations
                 WHERE name = 'source_022_file_partition_index_repair'",
                [],
            )?;
            source_db::checkpoint_source_db(&source_conn)?;
            drop(source_conn);
            case_conn.execute(
                "UPDATE data_sources
                 SET schema_version = 'source_021_cephfs_assembly_capability'
                 WHERE id = ?1",
                [&source.id.0],
            )?;
            Ok(())
        })
        .unwrap();
    drop(active);

    let reopened = case_service::open_case(&case_root).unwrap();
    reopened
        .with_conn(|case_conn| {
            let updated = DataSourceRepo::new(case_conn)
                .find_storage(&source.id)?
                .expect("source storage metadata");
            assert_eq!(
                updated.schema_version.as_deref(),
                Some(persistence_sqlite::runner::latest_source_version())
            );
            Ok(())
        })
        .unwrap();

    let source_conn = source_db::open_source_db(&case_root, &source.id).unwrap();
    let schema_version = persistence_sqlite::runner::current_version(&source_conn).unwrap();
    assert_eq!(
        schema_version.as_deref(),
        Some(persistence_sqlite::runner::latest_source_version())
    );
    for (id, expected) in [("root-2", 2_i64), ("etc", 2_i64), ("passwd", 2_i64)] {
        let partition_index: Option<i64> = source_conn
            .query_row(
                "SELECT partition_index FROM file_entries WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(partition_index, Some(expected), "{id}");
    }
}

#[test]
fn case_platform_validation_rejects_retired_macos_without_blocking_deletion() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(&tmp.path().join("cases"), "retired-macos", None)
        .expect("create case");
    let case_root = active.case_root.clone();
    let source = test_data_source(
        "ds-retired-macos",
        "retired-macos",
        tmp.path().join("retired-macos.e01"),
    );
    active
        .with_conn(|conn| {
            seed_isolated_source(conn, &case_root, &active.meta.id, &source, "macos")?;
            FileRepo::new(conn).insert_batch(&[FileEntry {
                id: FileEntryId("legacy-app-file".to_string()),
                parent_id: None,
                data_source_id: source.id.clone(),
                path: "legacy.txt".to_string(),
                name: "legacy.txt".to_string(),
                entry_type: EntryType::File,
                size: Some(1),
                ext: Some("txt".to_string()),
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
                read_only: false,
                archive: false,
                unix_mode: None,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            }])?;
            Ok(())
        })
        .expect("seed retired platform");

    let error = case_service::ensure_supported_data_source_platforms(&active)
        .expect_err("retired platform must fail closed");
    assert!(matches!(
        error,
        case_service::CaseServiceError::UnsupportedPlatform(ref platform)
            if platform == "macos"
    ));

    drop(active);
    let open_error = match case_service::open_case(&case_root) {
        Ok(_) => panic!("retired legacy case unexpectedly opened"),
        Err(error) => error,
    };
    assert!(matches!(
        open_error,
        case_service::CaseServiceError::UnsupportedPlatform(ref platform)
            if platform == "macos"
    ));
    let conn = app_services::connection::open_case_db(&case_root.join("app.db"))
        .expect("open case database for audit assertion");
    assert_eq!(
        AuditRepo::new(&conn)
            .count_by_action("case.open")
            .expect("count case open audit"),
        0
    );
    drop(conn);
    case_service::delete_case_in(&case_root).expect("unsupported case must remain deletable");
    assert!(!case_root.exists());
}

#[test]
fn case_platform_validation_rejects_unknown_and_blank_platforms() {
    for (case_name, platform, expected) in [
        ("unknown-platform", "unknown", "unknown"),
        ("blank-platform", "   ", "missing platform metadata"),
    ] {
        let tmp = TempDir::new().unwrap();
        let active = case_service::create_case(tmp.path(), case_name, None).expect("create case");
        let case_root = active.case_root.clone();
        let source = test_data_source(
            &format!("ds-{case_name}"),
            case_name,
            tmp.path().join(format!("{case_name}.e01")),
        );
        active
            .with_conn(|conn| {
                seed_isolated_source(conn, &case_root, &active.meta.id, &source, platform)?;
                Ok(())
            })
            .expect("seed unsupported platform");
        drop(active);

        let error = match case_service::open_case(&case_root) {
            Ok(_) => panic!("case with platform {platform:?} unexpectedly opened"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            case_service::CaseServiceError::UnsupportedPlatform(ref value)
                if value == expected
        ));
    }
}

#[test]
fn open_case_rejects_stale_schema_without_running_migrations() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(tmp.path(), "stale-schema", None).expect("create case");
    let case_root = active.case_root.clone();
    active
        .with_conn(|conn| {
            conn.execute(
                "DELETE FROM schema_migrations WHERE name = ?1",
                [persistence_sqlite::runner::latest_version()],
            )?;
            Ok(())
        })
        .expect("mark schema stale");
    drop(active);

    let error = match case_service::open_case(&case_root) {
        Ok(_) => panic!("stale schema unexpectedly opened"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        case_service::CaseServiceError::InvalidCaseDir(ref message)
            if message.contains("schema is incompatible") && message.contains("re-import")
    ));

    let conn = rusqlite::Connection::open(case_root.join("app.db")).expect("reopen case database");
    let latest_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE name = ?1",
            [persistence_sqlite::runner::latest_version()],
            |row| row.get(0),
        )
        .expect("check latest migration marker");
    assert_eq!(
        latest_count, 0,
        "case open must not migrate incompatible cases"
    );
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
fn delete_case_removes_valid_case_directory_outside_default_root() {
    let _guard = lock_case_env();
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("outside-cases");
    let active = case_service::create_case(&parent, "delete-me", Some("tester")).unwrap();
    let case_root = active.case_root.clone();
    let db_path = active.db_path();

    let delete_audit_count = active
        .with_conn(|conn| AuditRepo::new(conn).count_by_action("case.delete"))
        .unwrap();
    assert_eq!(delete_audit_count, 0);
    drop(active);

    assert!(case_root.join("case.json").is_file());
    assert!(case_root.join("app.db").is_file());

    case_service::delete_case(&case_root).unwrap();

    assert!(!case_root.exists());
    assert!(parent.exists());

    let reopened = case_service::open_case(&case_root);
    assert!(reopened.is_err());
    let delete_db = persistence_sqlite::open_existing(&db_path);
    assert!(delete_db.is_err());
}

#[test]
fn delete_case_rejects_non_case_directory_without_removing_it() {
    let _guard = lock_case_env();
    let tmp = TempDir::new().unwrap();
    std::env::set_var("APPDATA", tmp.path());
    let not_case = tmp.path().join("not-a-case");
    std::fs::create_dir_all(&not_case).unwrap();
    std::fs::write(not_case.join("note.txt"), "keep").unwrap();

    let result = case_service::delete_case(&not_case);

    assert!(result.is_err());
    assert!(not_case.exists());
    assert!(not_case.join("note.txt").is_file());
}

#[test]
fn delete_data_source_in_removes_isolated_storage_and_writes_audit_log() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "delete-ds", Some("tester")).unwrap();
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();
    let deleted_source =
        test_data_source("ds-delete", "delete-me", tmp.path().join("evidence-delete"));
    let retained_source =
        test_data_source("ds-retain", "retain-me", tmp.path().join("evidence-retain"));

    active
        .with_conn(|conn| {
            let deleted_paths =
                seed_isolated_source(conn, &case_root, &case_id, &deleted_source, "linux")?;
            let retained_paths =
                seed_isolated_source(conn, &case_root, &case_id, &retained_source, "windows")?;

            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                0
            );

            case_service::delete_data_source_in(conn, &case_root, &deleted_source.id.0).unwrap();

            let repo = DataSourceRepo::new(conn);
            assert!(repo.find_storage(&deleted_source.id)?.is_none());
            assert!(repo.find_storage(&retained_source.id)?.is_some());
            let remaining_sources = repo.find_by_case(&case_id)?;
            assert_eq!(remaining_sources.len(), 1);
            assert_eq!(remaining_sources[0].id, retained_source.id);

            assert!(!deleted_paths.source_dir.exists());
            assert!(!deleted_paths.staging_dir.exists());
            assert!(retained_paths.source_dir.join("source.db").is_file());
            assert!(retained_paths
                .source_dir
                .join("index")
                .join("index.marker")
                .is_file());
            assert!(retained_paths.staging_dir.join("staging.marker").is_file());

            let retained_conn =
                source_db::open_registered_source_db(conn, &case_root, &retained_source.id)?;
            for table in ["file_entries", "artifacts", "timeline_events"] {
                let count: i64 = retained_conn.query_row(
                    &format!("SELECT COUNT(*) FROM {table}"),
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(count, 1, "{table} for the retained source must survive");
            }

            assert_eq!(
                std::fs::read(&deleted_paths.evidence_marker)?,
                b"immutable evidence fixture"
            );
            assert_eq!(
                std::fs::read(&retained_paths.evidence_marker)?,
                b"immutable evidence fixture"
            );
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                1
            );

            Ok(())
        })
        .unwrap();
}

#[test]
fn delete_data_source_in_rejects_unknown_id_without_audit() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(&tmp.path().join("cases"), "unknown-ds", None).unwrap();
    let case_root = active.case_root.clone();

    active
        .with_conn(|conn| {
            let result = case_service::delete_data_source_in(conn, &case_root, "missing-source");
            assert!(result.is_err());
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                0
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn delete_data_source_in_retries_committed_tombstone_cleanup() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "cleanup-retry", None).unwrap();
    let case_root = active.case_root.clone();
    let data_source_id = "ds-cleanup-retry";
    let tombstone = case_root
        .join("cache")
        .join("data-source-tombstones")
        .join(data_source_id);
    std::fs::create_dir_all(&tombstone).unwrap();
    std::fs::write(tombstone.join("pending.marker"), b"pending cleanup").unwrap();

    active
        .with_conn(|conn| {
            case_service::delete_data_source_in(conn, &case_root, data_source_id)
                .expect("orphaned tombstone cleanup should be retryable");
            assert!(!tombstone.exists());
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                0
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn delete_data_source_in_clears_empty_precommit_tombstone_and_continues() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "empty-tombstone-retry", None)
            .unwrap();
    let case_root = active.case_root.clone();
    let source = test_data_source(
        "ds-empty-tombstone",
        "empty-tombstone",
        tmp.path().join("evidence-empty-tombstone"),
    );

    active
        .with_conn(|conn| {
            let paths = seed_isolated_source(conn, &case_root, &active.meta.id, &source, "linux")?;
            let tombstone = case_root
                .join("cache")
                .join("data-source-tombstones")
                .join(&source.id.0);
            std::fs::create_dir_all(&tombstone)?;

            case_service::delete_data_source_in(conn, &case_root, &source.id.0)
                .expect("an empty rollback tombstone should be cleaned before retrying deletion");

            assert!(!tombstone.exists());
            assert!(!paths.source_dir.exists());
            assert!(!paths.staging_dir.exists());
            assert!(DataSourceRepo::new(conn)
                .find_storage(&source.id)?
                .is_none());
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                1
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn data_source_delete_recovery_errors_expose_safe_recoverable_ipc_metadata() {
    let recovery = transport::CommandError::from_typed_service_error(
        case_service::CaseServiceError::DataSourceDeleteRecoveryPending {
            data_source_id: "ds-recovery".to_string(),
            tombstone: "cache/data-source-tombstones/ds-recovery".to_string(),
            reason: "private diagnostic remains in backend logs".to_string(),
        },
    );
    assert_eq!(recovery.code, "DATA_SOURCE_DELETE_RECOVERY_PENDING");
    assert_eq!(recovery.recoverable, Some(true));
    assert_eq!(
        recovery
            .details
            .as_ref()
            .and_then(|value| value.get("registrationDeleted"))
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        recovery
            .details
            .as_ref()
            .and_then(|value| value.get("tombstone"))
            .and_then(Value::as_str),
        Some("cache/data-source-tombstones/ds-recovery")
    );
    assert!(!recovery.message.contains("private diagnostic"));

    let cleanup = transport::CommandError::from_typed_service_error(
        case_service::CaseServiceError::DataSourceDeleteCleanupPending {
            data_source_id: "ds-cleanup".to_string(),
            tombstone: "cache/data-source-tombstones/ds-cleanup".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "host path"),
        },
    );
    assert_eq!(cleanup.code, "DATA_SOURCE_DELETE_CLEANUP_PENDING");
    assert_eq!(cleanup.recoverable, Some(true));
    assert_eq!(
        cleanup
            .details
            .as_ref()
            .and_then(|value| value.get("registrationDeleted"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert!(!cleanup.message.contains("host path"));

    let rollback = transport::CommandError::from_typed_service_error(
        case_service::CaseServiceError::DataSourceDeleteRollbackFailed {
            data_source_id: "ds-rollback".to_string(),
            tombstone: "cache/data-source-tombstones/ds-rollback".to_string(),
            step: "cleanupEmptyTombstone",
            original: Box::new(case_service::CaseServiceError::InvalidCaseDir(
                "path reconstruction remains private".to_string(),
            )),
            rollback: std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private host path",
            ),
        },
    );
    assert_eq!(rollback.code, "DATA_SOURCE_DELETE_ROLLBACK_FAILED");
    assert_eq!(rollback.category, "io");
    assert_eq!(rollback.recoverable, Some(true));
    assert_eq!(
        rollback
            .details
            .as_ref()
            .and_then(|value| value.get("rollbackStep"))
            .and_then(Value::as_str),
        Some("cleanupEmptyTombstone")
    );
    assert!(!rollback.message.contains("private host path"));
    assert!(!rollback.message.contains("path reconstruction"));
}

#[test]
fn delete_data_source_without_case_root_never_partially_deletes_registered_source() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "registration-only-delete", None)
            .unwrap();
    let source = test_data_source(
        "ds-registration-only",
        "registration-only",
        tmp.path().join("original-evidence"),
    );

    active
        .with_conn(|conn| {
            assert!(case_service::delete_data_source(conn, "missing-source").is_err());
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                0
            );

            std::fs::create_dir_all(&source.source_path)?;
            let evidence_marker = source.source_path.join("evidence.marker");
            std::fs::write(&evidence_marker, b"do not delete")?;
            DataSourceRepo::new(conn).insert(&active.meta.id, &source)?;

            assert!(case_service::delete_data_source(conn, &source.id.0).is_err());
            assert!(DataSourceRepo::new(conn)
                .find_storage(&source.id)?
                .is_some());
            assert_eq!(std::fs::read(evidence_marker)?, b"do not delete");
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                0
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn delete_data_source_in_rejects_invalid_managed_path_without_mutation() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "invalid-ds-path", None).unwrap();
    let case_root = active.case_root.clone();
    let source = test_data_source(
        "ds-invalid-path",
        "invalid-path",
        tmp.path().join("original-evidence"),
    );

    active
        .with_conn(|conn| {
            let mut storage = DataSourceStorage::source_db(&source.id.0, Some("linux"), None);
            storage.source_db_rel_path = Some("evidence/decoy/source.db".to_string());
            DataSourceRepo::new(conn).insert_with_storage(&active.meta.id, &source, &storage)?;

            let decoy_dir = case_root.join("evidence").join("decoy");
            std::fs::create_dir_all(&decoy_dir)?;
            std::fs::write(decoy_dir.join("source.db"), b"case evidence decoy")?;
            std::fs::create_dir_all(&source.source_path)?;
            let evidence_marker = source.source_path.join("evidence.marker");
            std::fs::write(&evidence_marker, b"original evidence")?;

            assert!(case_service::delete_data_source_in(conn, &case_root, &source.id.0).is_err());
            assert!(DataSourceRepo::new(conn)
                .find_storage(&source.id)?
                .is_some());
            assert_eq!(
                std::fs::read(decoy_dir.join("source.db"))?,
                b"case evidence decoy"
            );
            assert_eq!(std::fs::read(evidence_marker)?, b"original evidence");
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                0
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn delete_data_source_in_rejects_managed_path_overlapping_original_evidence() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "overlap-evidence", None).unwrap();
    let case_root = active.case_root.clone();
    let source_id = DataSourceId("ds-overlap-evidence".to_string());
    let source = test_data_source(
        &source_id.0,
        "overlap-evidence",
        source_db::source_dir(&case_root, &source_id),
    );

    active
        .with_conn(|conn| {
            let paths = seed_isolated_source(conn, &case_root, &active.meta.id, &source, "linux")?;
            let error = case_service::delete_data_source_in(conn, &case_root, &source.id.0)
                .expect_err("managed storage must never overlap original evidence");
            assert!(error.to_string().contains("overlaps"));
            assert!(DataSourceRepo::new(conn)
                .find_storage(&source.id)?
                .is_some());
            assert_seeded_source_survives(&paths);
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                0
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn delete_data_source_in_staging_failure_preserves_registration_and_data() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "stage-failure", None).unwrap();
    let case_root = active.case_root.clone();
    let source = test_data_source(
        "ds-stage-failure",
        "stage-failure",
        tmp.path().join("original-evidence"),
    );

    active
        .with_conn(|conn| {
            let paths = seed_isolated_source(conn, &case_root, &active.meta.id, &source, "linux")?;
            std::fs::write(
                case_root.join("cache").join("data-source-tombstones"),
                b"blocks tombstone directory creation",
            )?;

            assert!(case_service::delete_data_source_in(conn, &case_root, &source.id.0).is_err());
            assert!(DataSourceRepo::new(conn)
                .find_storage(&source.id)?
                .is_some());
            assert_seeded_source_survives(&paths);
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                0
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn delete_data_source_in_audit_failure_rolls_back_db_and_restores_paths() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "audit-failure", None).unwrap();
    let case_root = active.case_root.clone();
    let source = test_data_source(
        "ds-audit-failure",
        "audit-failure",
        tmp.path().join("original-evidence"),
    );

    active
        .with_conn(|conn| {
            let paths = seed_isolated_source(conn, &case_root, &active.meta.id, &source, "linux")?;
            conn.execute_batch(
                "CREATE TEMP TRIGGER fail_datasource_delete_audit
                 BEFORE INSERT ON audit_log
                 WHEN NEW.action = 'datasource.delete'
                 BEGIN
                     SELECT RAISE(ABORT, 'injected audit failure');
                 END;",
            )?;

            let result = case_service::delete_data_source_in(conn, &case_root, &source.id.0);
            assert!(result.is_err());
            assert!(DataSourceRepo::new(conn)
                .find_storage(&source.id)?
                .is_some());
            assert_seeded_source_survives(&paths);
            assert!(!case_root
                .join("cache")
                .join("data-source-tombstones")
                .join(&source.id.0)
                .exists());
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                0
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn delete_data_source_in_detects_precommit_tombstone_without_losing_staged_data() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "pending-delete", None).unwrap();
    let case_root = active.case_root.clone();
    let source = test_data_source(
        "ds-pending-delete",
        "pending-delete",
        tmp.path().join("original-evidence"),
    );

    active
        .with_conn(|conn| {
            let paths = seed_isolated_source(conn, &case_root, &active.meta.id, &source, "linux")?;
            let tombstone = case_root
                .join("cache")
                .join("data-source-tombstones")
                .join(&source.id.0);
            std::fs::create_dir_all(&tombstone)?;
            std::fs::rename(&paths.source_dir, tombstone.join("source"))?;
            std::fs::rename(&paths.staging_dir, tombstone.join("staging"))?;

            let error = case_service::delete_data_source_in(conn, &case_root, &source.id.0)
                .expect_err("pre-commit tombstone must require recovery");
            assert!(error.to_string().contains("requires recovery"));
            assert!(DataSourceRepo::new(conn)
                .find_storage(&source.id)?
                .is_some());
            assert!(tombstone.join("source").join("source.db").is_file());
            assert!(tombstone
                .join("source")
                .join("index")
                .join("index.marker")
                .is_file());
            assert!(tombstone.join("staging").join("staging.marker").is_file());
            assert_eq!(
                std::fs::read(&paths.evidence_marker)?,
                b"immutable evidence fixture"
            );
            assert_eq!(
                AuditRepo::new(conn).count_by_action("datasource.delete")?,
                0
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn no_running_jobs_drains_immediately() {
    let (conn, case_id) = setup_job_db();
    let result = case_service::close_case_drain(&conn, &case_id, 5000).unwrap();
    assert!(result.fully_drained);
    assert!(result.pending_jobs.is_empty());
    assert!(result.warnings.is_empty());
}

#[test]
fn drain_timeout_marks_jobs_interrupted() {
    let (conn, case_id) = setup_job_db();
    let repo = JobRepo::new(&conn);
    let running_id = repo.create(&case_id, "import").unwrap();
    let cancelling_id = repo.create(&case_id, "import").unwrap();
    repo.mark_cancelling(&cancelling_id, "Cancel requested")
        .unwrap();
    let completed_id = repo.create(&case_id, "index").unwrap();
    repo.complete(&completed_id, "done").unwrap();

    let result = case_service::close_case_drain(&conn, &case_id, 5000).unwrap();
    assert!(!result.fully_drained);
    assert_eq!(result.pending_jobs.len(), 2);
    assert!(result.pending_jobs.contains(&running_id.0));
    assert!(result.pending_jobs.contains(&cancelling_id.0));
    assert_eq!(result.warnings.len(), 2);

    let jobs = JobRepo::new(&conn).list_recent(10).unwrap();
    let running = jobs.iter().find(|job| job.id == running_id).unwrap();
    assert_eq!(running.status, "failed");
    assert!(running.detail.contains("interrupted_during_close"));
    let cancelling = jobs.iter().find(|job| job.id == cancelling_id).unwrap();
    assert_eq!(cancelling.status, "failed");
    assert!(cancelling.detail.contains("interrupted_during_close"));
    let completed = jobs.iter().find(|job| job.id == completed_id).unwrap();
    assert_eq!(completed.status, "completed");
}

#[test]
fn drain_completes_when_jobs_finish_quickly() {
    let (conn, case_id) = setup_job_db();
    let repo = JobRepo::new(&conn);
    let job_id = repo.create(&case_id, "quick-task").unwrap();
    repo.complete(&job_id, "finished quickly").unwrap();

    let result = case_service::close_case_drain(&conn, &case_id, 5000).unwrap();
    assert!(result.fully_drained);
    assert!(result.pending_jobs.is_empty());
    assert!(result.warnings.is_empty());
    let jobs = JobRepo::new(&conn).list_recent(10).unwrap();
    assert_eq!(
        jobs.iter().find(|job| job.id == job_id).unwrap().status,
        "completed"
    );
}

#[test]
fn open_case_rejects_legacy_single_database_payloads() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(tmp.path(), "legacy_case", Some("tester")).unwrap();
    let case_root = active.case_root.clone();

    active
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO data_sources
                 (id, case_id, name, kind, source_path, storage_model, platform)
                 VALUES ('legacy-ds', ?1, 'Legacy source', 'logical_directory', 'D:/legacy', 'source_db', 'windows')",
                [&active.meta.id.0],
            )?;
            conn.execute(
                "INSERT INTO file_entries
                 (id, parent_id, data_source_id, path, name, entry_type, size, deleted, hidden, system)
                 VALUES ('legacy-file', NULL, 'legacy-ds', '/', '/', 'directory', NULL, 0, 0, 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    drop(active);

    let error = match case_service::open_case(&case_root) {
        Ok(_) => panic!("legacy app.db payload should be rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("legacy single-database"));
    assert!(error.to_string().contains("re-import is required"));
}
