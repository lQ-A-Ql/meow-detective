use super::*;
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};

#[test]
fn global_file_id_round_trips() {
    let global = GlobalFileId::new(
        DataSourceId("ds-1".to_string()),
        FileEntryId("mft:0:42".to_string()),
    );

    let encoded = global.encode();
    assert_eq!(encoded.0, "ds:ds-1:mft:0:42");
    assert_eq!(GlobalFileId::parse(&encoded).unwrap(), global);
}

#[test]
fn global_file_id_rejects_unscoped_ids() {
    let err = GlobalFileId::parse(&FileEntryId("mft:0:42".to_string())).unwrap_err();

    assert!(err.to_string().contains("not a source-scoped id"));
}

#[test]
fn global_file_id_rejects_unsafe_source_ids() {
    for value in [
        "ds:../outside:mft:0:42",
        "ds:bad/source:mft:0:42",
        "ds:bad source:mft:0:42",
    ] {
        let err = GlobalFileId::parse(&FileEntryId(value.to_string())).unwrap_err();
        assert!(err.to_string().contains("invalid source id"));
    }
}

#[test]
fn safe_case_relative_path_rejects_escape_paths() {
    let case_root = Path::new("D:/cases/case-1");

    for rel_path in ["../outside/source.db", "/tmp/source.db", "C:/tmp/source.db"] {
        let err = safe_case_relative_path(case_root, rel_path).unwrap_err();
        assert!(err.to_string().contains("escapes the case directory"));
    }

    assert_eq!(
        safe_case_relative_path(case_root, "sources/ds-1/source.db").unwrap(),
        case_root.join("sources/ds-1/source.db")
    );
}

#[test]
fn safe_existing_case_path_rejects_canonical_escape() {
    let case_root = tempfile::TempDir::new().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    let inside = case_root.path().join("source.db");
    std::fs::write(&inside, b"sqlite").unwrap();

    let allowed = safe_existing_case_path(case_root.path(), &inside).unwrap();
    assert!(allowed.starts_with(case_root.path().canonicalize().unwrap()));

    let err = safe_existing_case_path(case_root.path(), outside.path()).unwrap_err();
    assert!(err.to_string().contains("escapes the case directory"));
}

#[test]
fn registered_source_index_dir_requires_a_safe_registered_path() {
    let tmp = tempfile::TempDir::new().unwrap();
    let case_conn = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&case_conn).unwrap();
    case_conn
        .execute("INSERT INTO cases (id, name) VALUES ('case-1', 'Case')", [])
        .unwrap();
    let ds = domain::DataSource {
        id: DataSourceId("ds-index".to_string()),
        name: "Indexed Source".to_string(),
        kind: domain::DataSourceKind::Raw,
        source_path: std::path::PathBuf::from("D:/source.raw"),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&ds.id.0, Some("linux"), None);
    storage.index_rel_path = None;
    DataSourceRepo::new(&case_conn)
        .insert_with_storage(&domain::CaseId("case-1".to_string()), &ds, &storage)
        .unwrap();

    let missing = registered_source_index_dir(&case_conn, tmp.path(), &ds.id).unwrap_err();
    assert!(missing.to_string().contains("missing search index path"));

    case_conn
        .execute(
            "UPDATE data_sources SET index_rel_path='../outside-index' WHERE id=?1",
            [&ds.id.0],
        )
        .unwrap();
    let escaped = registered_source_index_dir(&case_conn, tmp.path(), &ds.id).unwrap_err();
    assert!(escaped.to_string().contains("escapes the case directory"));

    case_conn
        .execute(
            "UPDATE data_sources SET index_rel_path='search/custom-index' WHERE id=?1",
            [&ds.id.0],
        )
        .unwrap();
    assert_eq!(
        registered_source_index_dir(&case_conn, tmp.path(), &ds.id).unwrap(),
        tmp.path().join("search/custom-index")
    );
}

#[test]
fn open_registered_source_db_rejects_missing_source_db() {
    let tmp = tempfile::TempDir::new().unwrap();
    let case_conn = persistence_sqlite::connection::open_in_memory().unwrap();
    case_conn
        .execute_batch(
            "CREATE TABLE data_sources (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                source_path TEXT NOT NULL,
                imported_at TEXT NOT NULL DEFAULT (datetime('now')),
                source_hash_sha256 TEXT,
                hash_status TEXT DEFAULT 'unknown',
                canonical_source_path TEXT,
                evidence_size INTEGER,
                reader_kind TEXT,
                provenance_status TEXT DEFAULT 'unknown',
                provenance_warnings TEXT DEFAULT '[]',
                storage_model TEXT NOT NULL DEFAULT 'source_db',
                source_db_rel_path TEXT,
                index_rel_path TEXT,
                staging_rel_path TEXT,
                platform TEXT NOT NULL DEFAULT 'unknown',
                profile TEXT,
                import_state TEXT NOT NULL DEFAULT 'pending',
                schema_version TEXT,
                last_error TEXT
            );",
        )
        .unwrap();
    let ds = domain::DataSource {
        id: DataSourceId("ds-missing".to_string()),
        name: "Missing Source".to_string(),
        kind: domain::DataSourceKind::Raw,
        source_path: std::path::PathBuf::from("D:/missing.raw"),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    DataSourceRepo::new(&case_conn)
        .insert_with_storage(
            &domain::CaseId("case-1".to_string()),
            &ds,
            &DataSourceStorage::source_db(&ds.id.0, Some("linux"), None),
        )
        .unwrap();

    let err = open_registered_source_db(&case_conn, tmp.path(), &ds.id).unwrap_err();

    assert!(err.to_string().contains("source DB is missing"));
}

#[test]
fn open_registered_source_db_migrates_schema_version_mismatch() {
    let tmp = tempfile::TempDir::new().unwrap();
    let case_conn = persistence_sqlite::connection::open_in_memory().unwrap();
    case_conn
        .execute_batch(
            "CREATE TABLE data_sources (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                source_path TEXT NOT NULL,
                imported_at TEXT NOT NULL DEFAULT (datetime('now')),
                source_hash_sha256 TEXT,
                hash_status TEXT DEFAULT 'unknown',
                canonical_source_path TEXT,
                evidence_size INTEGER,
                reader_kind TEXT,
                provenance_status TEXT DEFAULT 'unknown',
                provenance_warnings TEXT DEFAULT '[]',
                storage_model TEXT NOT NULL DEFAULT 'source_db',
                source_db_rel_path TEXT,
                index_rel_path TEXT,
                staging_rel_path TEXT,
                platform TEXT NOT NULL DEFAULT 'unknown',
                profile TEXT,
                import_state TEXT NOT NULL DEFAULT 'pending',
                schema_version TEXT,
                last_error TEXT
            );",
        )
        .unwrap();
    let ds = domain::DataSource {
        id: DataSourceId("ds-old-schema".to_string()),
        name: "Old schema".to_string(),
        kind: domain::DataSourceKind::Raw,
        source_path: std::path::PathBuf::from("D:/old.raw"),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&ds.id.0, Some("linux"), None);
    storage.schema_version = Some("source_000_legacy".to_string());
    DataSourceRepo::new(&case_conn)
        .insert_with_storage(&domain::CaseId("case-1".to_string()), &ds, &storage)
        .unwrap();
    crate::source_db::open_source_db(tmp.path(), &ds.id).unwrap();

    let connection = open_registered_source_db(&case_conn, tmp.path(), &ds.id).unwrap();

    assert!(connection
        .prepare("SELECT 1 FROM ceph_osd_inventory LIMIT 1")
        .is_ok());
    assert!(connection
        .prepare("SELECT 1 FROM ceph_bluefs_superblocks LIMIT 1")
        .is_ok());
    let updated = DataSourceRepo::new(&case_conn)
        .find_storage(&ds.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.schema_version.as_deref(),
        Some(persistence_sqlite::runner::latest_source_version())
    );
}

#[test]
fn reconstruction_route_accepts_ready_metadata_but_ready_route_does_not() {
    let tmp = tempfile::TempDir::new().unwrap();
    let case_conn = persistence_sqlite::connection::open_in_memory().unwrap();
    case_conn
        .execute_batch(
            "CREATE TABLE data_sources (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                source_path TEXT NOT NULL,
                imported_at TEXT NOT NULL DEFAULT (datetime('now')),
                source_hash_sha256 TEXT,
                hash_status TEXT DEFAULT 'unknown',
                canonical_source_path TEXT,
                evidence_size INTEGER,
                reader_kind TEXT,
                provenance_status TEXT DEFAULT 'unknown',
                provenance_warnings TEXT DEFAULT '[]',
                storage_model TEXT NOT NULL DEFAULT 'source_db',
                source_db_rel_path TEXT,
                index_rel_path TEXT,
                staging_rel_path TEXT,
                platform TEXT NOT NULL DEFAULT 'unknown',
                profile TEXT,
                import_state TEXT NOT NULL DEFAULT 'pending',
                schema_version TEXT,
                last_error TEXT
            );",
        )
        .unwrap();
    let case_id = domain::CaseId("case-1".to_string());
    let ds = domain::DataSource {
        id: DataSourceId("ds-metadata".to_string()),
        name: "Metadata source".to_string(),
        kind: domain::DataSourceKind::E01,
        source_path: std::path::PathBuf::from("D:/metadata.E01"),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&ds.id.0, Some("linux"), None);
    storage.import_state = "ready_metadata".to_string();
    DataSourceRepo::new(&case_conn)
        .insert_with_storage(&case_id, &ds, &storage)
        .unwrap();
    open_source_db(tmp.path(), &ds.id).unwrap();

    let reconstruction =
        open_reconstruction_source_by_id(&case_conn, tmp.path(), &case_id, &ds.id).unwrap();
    assert_eq!(reconstruction.data_source_id, ds.id);
    assert_eq!(reconstruction.platform, domain::DataSourcePlatform::Linux);
    assert!(matches!(
        open_ready_source_by_id(&case_conn, tmp.path(), &case_id, &ds.id),
        Err(ReadySourceError::NotReady { .. })
    ));
}

#[test]
fn checkpoint_source_db_requires_wal_convergence() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("source.db");
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer.pragma_update(None, "journal_mode", "WAL").unwrap();
    writer
        .execute_batch("CREATE TABLE records(value INTEGER); INSERT INTO records VALUES (1);")
        .unwrap();

    checkpoint_source_db(&writer).unwrap();

    let (busy, log_frames, checkpointed_frames): (u32, u64, u64) = writer
        .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    assert_eq!(busy, 0);
    assert_eq!(log_frames, checkpointed_frames);
}

#[test]
fn checkpoint_source_db_fails_when_a_reader_pins_wal_frames() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("source.db");
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer.pragma_update(None, "journal_mode", "WAL").unwrap();
    writer
        .execute_batch("CREATE TABLE records(value INTEGER); INSERT INTO records VALUES (1);")
        .unwrap();

    let reader = rusqlite::Connection::open(&path).unwrap();
    let read_tx = reader.unchecked_transaction().unwrap();
    let _: i64 = read_tx
        .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
        .unwrap();
    writer
        .execute("INSERT INTO records VALUES (2)", [])
        .unwrap();

    let error = checkpoint_source_db(&writer).unwrap_err();
    assert!(error.to_string().contains("did not converge"));
    drop(read_tx);
}

#[test]
fn source_build_database_is_hidden_until_atomic_publish() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_source_id = DataSourceId("derived-build".to_string());
    let attempt_id = "11111111-1111-4111-8111-111111111111";
    let build_path =
        super::build::source_build_db_path(tmp.path(), &data_source_id, attempt_id).unwrap();
    let final_path = source_db_path(tmp.path(), &data_source_id);
    let connection = open_fresh_source_build_db(tmp.path(), &data_source_id, attempt_id).unwrap();
    let wal_autocheckpoint: u32 = connection
        .query_row("PRAGMA wal_autocheckpoint", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        wal_autocheckpoint,
        super::build::SOURCE_BUILD_WAL_AUTOCHECKPOINT_PAGES
    );
    connection
        .execute(
            "INSERT INTO source_meta (key, value) VALUES ('catalogState', 'complete')",
            [],
        )
        .unwrap();

    assert!(build_path.is_file());
    assert!(!final_path.exists());
    finalize_source_build_db(&connection).unwrap();
    drop(connection);
    let published = publish_source_build_db(tmp.path(), &data_source_id, attempt_id).unwrap();

    assert_eq!(published, final_path);
    assert!(final_path.is_file());
    assert!(!build_path.exists());
    let published_connection =
        persistence_sqlite::open_existing_source_read_only(&final_path).unwrap();
    assert_eq!(
        published_connection
            .query_row(
                "SELECT value FROM source_meta WHERE key = 'catalogState'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "complete"
    );
}

#[test]
fn fresh_source_build_refuses_to_replace_a_published_database() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_source_id = DataSourceId("derived-published".to_string());
    let published = open_source_db(tmp.path(), &data_source_id).unwrap();
    drop(published);

    let error = open_fresh_source_build_db(
        tmp.path(),
        &data_source_id,
        "22222222-2222-4222-8222-222222222222",
    )
    .unwrap_err();

    assert!(error.to_string().contains("already has a published"));
}

#[test]
fn controlled_discard_removes_unpublished_source_build_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_source_id = DataSourceId("derived-discard".to_string());
    let attempt_id = "33333333-3333-4333-8333-333333333333";
    let build_path =
        super::build::source_build_db_path(tmp.path(), &data_source_id, attempt_id).unwrap();
    let connection = open_fresh_source_build_db(tmp.path(), &data_source_id, attempt_id).unwrap();
    connection
        .execute(
            "INSERT INTO source_meta (key, value) VALUES ('catalogState', 'partial')",
            [],
        )
        .unwrap();
    drop(connection);

    discard_source_build_db(tmp.path(), &data_source_id, attempt_id).unwrap();

    assert!(!build_path.exists());
    assert!(!PathBuf::from(format!("{}-wal", build_path.display())).exists());
    assert!(!PathBuf::from(format!("{}-shm", build_path.display())).exists());
}

#[test]
fn source_build_attempts_use_isolated_physical_paths() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_source_id = DataSourceId("derived-attempt-isolation".to_string());
    let first_attempt = "44444444-4444-4444-8444-444444444444";
    let second_attempt = "55555555-5555-4555-8555-555555555555";

    let first = open_fresh_source_build_db(tmp.path(), &data_source_id, first_attempt).unwrap();
    let second = open_fresh_source_build_db(tmp.path(), &data_source_id, second_attempt).unwrap();
    let first_path =
        super::build::source_build_db_path(tmp.path(), &data_source_id, first_attempt).unwrap();
    let second_path =
        super::build::source_build_db_path(tmp.path(), &data_source_id, second_attempt).unwrap();

    assert_ne!(first_path, second_path);
    assert!(first_path.is_file());
    assert!(second_path.is_file());
    drop(first);
    drop(second);
    discard_source_build_db(tmp.path(), &data_source_id, first_attempt).unwrap();
    assert!(!first_path.exists());
    assert!(second_path.exists());
}
