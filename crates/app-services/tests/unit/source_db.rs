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
