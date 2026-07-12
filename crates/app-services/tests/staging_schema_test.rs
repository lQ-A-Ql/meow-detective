mod staging_support;

use app_services::staging::{
    analysis_staging_db_path, get_staging_meta, open_analysis_staging, open_enum_staging,
    open_partition_staging, set_staging_meta, staging_db_row_count, ImportPhase, PartitionStatus,
    StagingManifest,
};
use rusqlite::Connection;
use staging_support::done_partition;

#[test]
fn staging_manifest_serialization_roundtrip() {
    let manifest = StagingManifest::create("ds-1", "/evidence/disk.E01", "E01");
    assert_eq!(manifest.phase, ImportPhase::Enumerating);
    assert!(manifest.partitions.is_empty());

    let json = serde_json::to_string(&manifest).unwrap();
    let decoded: StagingManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.data_source_id, "ds-1");
}

#[test]
fn staging_manifest_pending_and_completion_state() {
    let mut manifest = StagingManifest::create("ds-1", "/evidence/disk.E01", "E01");
    manifest.partitions.push(done_partition(0, "P0", 100));
    let mut pending = done_partition(1, "P1", 0);
    pending.status = PartitionStatus::Pending;
    manifest.partitions.push(pending);

    assert_eq!(manifest.pending_partitions().len(), 1);
    assert!(!manifest.all_partitions_done());
    manifest.partitions[1].status = PartitionStatus::Done;
    assert!(manifest.all_partitions_done());
    assert!(!StagingManifest::create("empty", "/evidence/disk.E01", "E01").all_partitions_done());
}

#[test]
fn staging_manifest_atomic_save_and_load_roundtrip() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut manifest = StagingManifest::create("ds-save", "/evidence/test.E01", "E01");
    manifest
        .partitions
        .push(done_partition(0, "Partition 0", 42));
    manifest.save(tmp.path()).unwrap();

    let loaded = StagingManifest::load(tmp.path(), "ds-save").unwrap();
    assert_eq!(loaded.partitions.len(), 1);
    assert_eq!(loaded.partitions[0].file_count, 42);
}

#[test]
fn staging_schema_supports_rows_and_metadata() {
    let tmp = tempfile::TempDir::new().unwrap();
    let conn = open_partition_staging(tmp.path(), "ds-schema", 0).unwrap();
    conn.execute(
        "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
         VALUES ('f1', 'ds-schema', '/test/file.txt', 'file.txt', 'File')",
        [],
    )
    .unwrap();
    set_staging_meta(&conn, "status", "done").unwrap();

    assert_eq!(staging_db_row_count(&conn).unwrap(), 1);
    assert_eq!(
        get_staging_meta(&conn, "status").unwrap().as_deref(),
        Some("done")
    );
    assert_eq!(get_staging_meta(&conn, "missing").unwrap(), None);
}

#[test]
fn staging_enum_bulk_schema_has_no_secondary_indexes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let conn = open_enum_staging(tmp.path(), "ds-index", 0).unwrap();
    let indexes: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index' ORDER BY name")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };

    for forbidden in [
        "idx_staging_parent",
        "idx_staging_path",
        "idx_staging_data_source",
    ] {
        assert!(!indexes.iter().any(|index| index == forbidden));
    }
}

#[test]
fn staging_connections_use_bounded_single_writer_pragmas() {
    let tmp = tempfile::TempDir::new().unwrap();
    let enum_conn = open_enum_staging(tmp.path(), "ds-pragmas", 0).unwrap();
    let analysis_conn = open_analysis_staging(tmp.path(), "ds-pragmas", 0).unwrap();

    for conn in [&enum_conn, &analysis_conn] {
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        let synchronous: i64 = conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        let temp_store: i64 = conn
            .query_row("PRAGMA temp_store", [], |row| row.get(0))
            .unwrap();
        let foreign_keys: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        let cache_size: i64 = conn
            .query_row("PRAGMA cache_size", [], |row| row.get(0))
            .unwrap();
        let locking_mode: String = conn
            .query_row("PRAGMA locking_mode", [], |row| row.get(0))
            .unwrap();

        assert_eq!(journal_mode, "wal");
        assert_eq!(synchronous, 0);
        assert_eq!(temp_store, 2);
        assert_eq!(foreign_keys, 1);
        assert_eq!(cache_size, -(16 * 1024));
        assert_eq!(locking_mode, "exclusive");
    }
}

#[test]
fn staging_analysis_schema_creates_expected_tables() {
    let tmp = tempfile::TempDir::new().unwrap();
    let conn = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
    for table in [
        "artifact_rows",
        "timeline_rows",
        "index_docs",
        "worker_meta",
    ] {
        let name: String = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, table);
    }
}

#[test]
fn staging_analysis_schema_upgrades_legacy_provenance_columns() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = analysis_staging_db_path(tmp.path(), "ds-analysis", 0);
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let legacy = Connection::open(&db_path).unwrap();
    legacy
        .execute_batch(
            "CREATE TABLE artifact_rows (
                id TEXT PRIMARY KEY NOT NULL, file_id TEXT, artifact_type TEXT NOT NULL,
                display_name TEXT NOT NULL, summary TEXT NOT NULL DEFAULT '',
                data_json TEXT NOT NULL DEFAULT '{}', source_path TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            );
            CREATE TABLE timeline_rows (
                id TEXT PRIMARY KEY NOT NULL, file_id TEXT NOT NULL, timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL, title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '', data_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE index_docs (
                file_id TEXT PRIMARY KEY NOT NULL, path TEXT NOT NULL, text TEXT NOT NULL,
                language TEXT NOT NULL DEFAULT 'unknown', truncated INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE worker_meta (
                key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL
            );",
        )
        .unwrap();
    drop(legacy);

    let upgraded = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
    for (table, column) in [
        ("artifact_rows", "extractor_id"),
        ("artifact_rows", "extractor_version"),
        ("artifact_rows", "confidence"),
        ("artifact_rows", "source_attribution"),
        ("timeline_rows", "parser_id"),
        ("timeline_rows", "parser_version"),
        ("timeline_rows", "confidence"),
        ("timeline_rows", "source_attribution"),
    ] {
        let present: bool = upgraded
            .query_row(
                &format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{table}') WHERE name = ?1"),
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert!(present, "missing upgraded column {table}.{column}");
    }
}
