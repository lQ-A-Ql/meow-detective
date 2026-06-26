//! Staging management for parallel import.
//!
//! Manages temporary per-partition databases during import:
//! - Manifest tracking (partition state, progress, resume cursor)
//! - Staging DB lifecycle (create, query, merge, cleanup)

mod analysis_merge;
mod cleanup;
mod db_paths;
mod enum_merge;
mod error;
mod manifest;
mod schema_bootstrap;

pub use analysis_merge::{merge_analysis_staging_to_main, AnalysisMergeStats};
pub use cleanup::cleanup_staging;
pub use db_paths::{analysis_staging_db_path, enum_staging_db_path, staging_db_path, staging_dir};
pub use enum_merge::{
    merge_all_staging_to_main, merge_all_staging_to_main_with_stats, StagingMergeStats,
};
pub use error::StagingError;
pub use manifest::{ImportPhase, PartitionEntry, PartitionStatus, StagingManifest};
pub use schema_bootstrap::{
    analysis_staging_counts, get_staging_meta, get_worker_meta, open_analysis_staging,
    open_enum_staging, open_partition_staging, set_staging_meta, set_worker_meta,
    staging_db_row_count,
};

#[cfg(test)]
use analysis_merge::{merge_one_analysis_index_docs, INDEX_DOC_MERGE_PAGE_SIZE};
#[cfg(test)]
use enum_merge::find_partition_placeholder_root_id_by_index;
#[cfg(test)]
use persistence_sqlite::repositories::staging_repo::table_has_column;
#[cfg(test)]
use rusqlite::{params, Connection};
#[cfg(test)]
use schema_bootstrap::STAGING_CACHE_SIZE_KIB;

fn rows_per_sec(rows: u64, duration: std::time::Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        rows
    } else {
        (rows as f64 / secs).round() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_create_and_serialize() {
        let m = StagingManifest::create("ds-1", "/evidence/disk.E01", "E01");
        assert_eq!(m.phase, ImportPhase::Enumerating);
        assert!(m.partitions.is_empty());

        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("ds-1"));
        let deserialized: StagingManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.data_source_id, "ds-1");
    }

    #[test]
    fn manifest_pending_partitions() {
        let mut m = StagingManifest::create("ds-1", "/evidence/disk.E01", "E01");
        m.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 100,
            dir_count: 10,
            total_size: 5000,
            last_path: None,
            completed_at: None,
            error: None,
        });
        m.partitions.push(PartitionEntry {
            index: 1,
            name: "P1".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_1.db".to_string(),
            status: PartitionStatus::Pending,
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });

        assert_eq!(m.pending_partitions().len(), 1);
        assert!(!m.all_partitions_done());
    }

    #[test]
    fn manifest_save_and_load_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut m = StagingManifest::create("ds-test", "/evidence/test.E01", "E01");
        m.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 42,
            dir_count: 5,
            total_size: 12345,
            last_path: None,
            completed_at: Some("2026-01-01T00:00:00Z".to_string()),
            error: None,
        });
        m.save(tmp.path()).unwrap();

        let loaded = StagingManifest::load(tmp.path(), "ds-test").unwrap();
        assert_eq!(loaded.partitions.len(), 1);
        assert_eq!(loaded.partitions[0].file_count, 42);
    }

    #[test]
    fn staging_db_create_and_insert() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_partition_staging(tmp.path(), "ds-1", 0).unwrap();

        conn.execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
             VALUES ('f1', 'ds-1', '/test/file.txt', 'file.txt', 'File')",
            [],
        )
        .unwrap();

        let count = staging_db_row_count(&conn).unwrap();
        assert_eq!(count, 1);

        set_staging_meta(&conn, "status", "done").unwrap();
        let status = get_staging_meta(&conn, "status").unwrap();
        assert_eq!(status.as_deref(), Some("done"));
    }

    #[test]
    fn enum_staging_bulk_schema_has_no_secondary_indexes_during_insert() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_enum_staging(tmp.path(), "ds-idx", 0).unwrap();

        let indexes: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='index' ORDER BY name")
                .unwrap();
            stmt.query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };

        assert!(!indexes.iter().any(|idx| idx == "idx_staging_parent"));
        assert!(!indexes.iter().any(|idx| idx == "idx_staging_path"));
        assert!(!indexes.iter().any(|idx| idx == "idx_staging_data_source"));
    }

    #[test]
    fn enum_and_analysis_staging_use_aggressive_temp_pragmas() {
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

            assert_eq!(journal_mode, "wal");
            assert_eq!(synchronous, 0);
            assert_eq!(temp_store, 2);
            assert_eq!(foreign_keys, 1);
            assert_eq!(cache_size, -STAGING_CACHE_SIZE_KIB);
        }
    }

    #[test]
    fn merge_staging_to_main() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        main_conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS file_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    parent_id TEXT,
                    data_source_id TEXT NOT NULL,
                    path TEXT NOT NULL,
                    name TEXT NOT NULL,
                    entry_type TEXT NOT NULL,
                    size INTEGER,
                    ext TEXT,
                    deleted INTEGER NOT NULL DEFAULT 0,
                    hidden INTEGER NOT NULL DEFAULT 0,
                    system INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT,
                    modified_at TEXT,
                    accessed_at TEXT,
                    changed_at TEXT,
                    hash_sha256 TEXT
                )",
            )
            .unwrap();

        // Create staging DB with some entries
        let ds_id = "ds-merge-test";
        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        for i in 0..5 {
            staging_conn
                .execute(
                    "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                     VALUES (?1, ?2, ?3, ?4, 'File')",
                    params![
                        format!("f{}", i),
                        ds_id,
                        format!("/test/file{}.txt", i),
                        format!("file{}.txt", i),
                    ],
                )
                .unwrap();
        }
        drop(staging_conn);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 5,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });

        let merged =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();
        assert_eq!(merged, 5);

        // 5 merged staging rows + 1 synthesized partition placeholder root.
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 6);

        // The synthesized root is promoted to a single first-level partition
        // root (path cleared, name set to the partition name) during merge.
        let root_count: i64 = main_conn
            .query_row(
                "SELECT COUNT(*) FROM file_entries WHERE parent_id IS NULL AND name = 'P0'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root_count, 1);

        let mixed_case_types: i64 = main_conn
            .query_row(
                "SELECT COUNT(*) FROM file_entries WHERE entry_type NOT IN ('file', 'directory')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mixed_case_types, 0);
    }

    fn create_main_file_entries_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS file_entries (
                id TEXT PRIMARY KEY NOT NULL,
                parent_id TEXT,
                data_source_id TEXT NOT NULL,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                entry_type TEXT NOT NULL,
                size INTEGER,
                ext TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                hidden INTEGER NOT NULL DEFAULT 0,
                system INTEGER NOT NULL DEFAULT 0,
                created_at TEXT,
                modified_at TEXT,
                accessed_at TEXT,
                changed_at TEXT,
                hash_sha256 TEXT
            )",
        )
        .unwrap();
    }

    fn make_done_partition(index: usize, file_count: u64) -> PartitionEntry {
        PartitionEntry {
            index,
            name: format!("P{}", index),
            fs_kind: "Ntfs".to_string(),
            staging_db: format!("partition_{}.db", index),
            status: PartitionStatus::Done,
            file_count,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        }
    }

    fn attached_db_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn.prepare("PRAGMA database_list").unwrap();
        stmt.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn merge_all_staging_two_partitions() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-two-part";

        // Partition 0: 3 files
        let s0 = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        for i in 0..3 {
            s0.execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES (?1, ?2, ?3, ?4, 'File')",
                params![
                    format!("p0f{}", i),
                    ds_id,
                    format!("/p0/file{}.txt", i),
                    format!("file{}.txt", i),
                ],
            )
            .unwrap();
        }

        // Partition 1: 2 files
        let s1 = open_partition_staging(tmp.path(), ds_id, 1).unwrap();
        for i in 0..2 {
            s1.execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES (?1, ?2, ?3, ?4, 'File')",
                params![
                    format!("p1f{}", i),
                    ds_id,
                    format!("/p1/file{}.txt", i),
                    format!("file{}.txt", i),
                ],
            )
            .unwrap();
        }
        drop(s0);
        drop(s1);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        for idx in 0..2 {
            manifest.partitions.push(PartitionEntry {
                index: idx,
                name: format!("P{}", idx),
                fs_kind: "Ntfs".to_string(),
                staging_db: format!("partition_{}.db", idx),
                status: PartitionStatus::Done,
                file_count: if idx == 0 { 3 } else { 2 },
                dir_count: 0,
                total_size: 0,
                last_path: None,
                completed_at: None,
                error: None,
            });
        }

        let merged =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();
        assert_eq!(merged, 5);

        // 5 merged staging rows + 1 synthesized placeholder root per partition (2).
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 7);

        // One first-level partition root per partition (promoted from the
        // synthesized placeholders; names set to P0/P1 during merge).
        let root_count: i64 = main_conn
            .query_row(
                "SELECT COUNT(*) FROM file_entries WHERE parent_id IS NULL AND name IN ('P0', 'P1')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root_count, 2);
    }

    #[test]
    fn merge_marks_staging_merged_and_repeat_is_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-idempotent";
        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        staging_conn
            .execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES ('f1', ?1, '/f1', 'f1', 'File')",
                params![ds_id],
            )
            .unwrap();
        drop(staging_conn);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(make_done_partition(0, 1));

        let first =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();
        let second =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 0);
        // 1 merged staging row + 1 synthesized placeholder root; second merge is
        // skipped (already merged) so no duplicate placeholder is created.
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        assert_eq!(
            get_staging_meta(&staging_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("true")
        );
    }

    #[test]
    fn merge_reports_conflicting_staging_rows_as_errors() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-conflict-accounting";
        main_conn
            .execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES ('existing', ?1, '/existing', 'existing', 'file')",
                params![ds_id],
            )
            .unwrap();

        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        staging_conn
            .execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES ('existing', ?1, '/staged-existing', 'staged-existing', 'File')",
                params![ds_id],
            )
            .unwrap();
        staging_conn
            .execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES ('new', ?1, '/new', 'new', 'File')",
                params![ds_id],
            )
            .unwrap();
        drop(staging_conn);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(make_done_partition(0, 2));

        let err =
            merge_all_staging_to_main_with_stats(&main_conn, tmp.path(), ds_id, &manifest, None)
                .unwrap_err();
        assert!(err.to_string().contains("Merge partition 0"));

        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let existing_path: String = main_conn
            .query_row(
                "SELECT path FROM file_entries WHERE id = 'existing'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(existing_path, "/existing");

        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        assert_ne!(
            get_staging_meta(&staging_conn, "merged")
                .unwrap()
                .as_deref(),
            Some("true")
        );
    }

    #[test]
    fn merge_skips_partition_already_marked_merged() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-skip-merged";
        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        staging_conn
            .execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES ('f1', ?1, '/f1', 'f1', 'File')",
                params![ds_id],
            )
            .unwrap();
        set_staging_meta(&staging_conn, "merged", "true").unwrap();
        drop(staging_conn);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(make_done_partition(0, 1));

        let merged =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();

        assert_eq!(merged, 0);
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn merge_failure_rolls_back_and_detaches_staging() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        main_conn
            .execute_batch(
                "CREATE TABLE file_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    data_source_id TEXT NOT NULL,
                    path TEXT NOT NULL,
                    name TEXT NOT NULL,
                    entry_type TEXT NOT NULL
                );
                INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                VALUES ('existing', 'ds', '/existing', 'existing', 'File');",
            )
            .unwrap();

        let ds_id = "ds-fail";
        let staging_conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        staging_conn
            .execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
                 VALUES ('f1', ?1, '/f1', 'f1', 'File')",
                params![ds_id],
            )
            .unwrap();
        drop(staging_conn);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(make_done_partition(0, 1));

        assert!(merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).is_err());
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(!attached_db_names(&main_conn)
            .iter()
            .any(|name| name == "staging"));
    }

    #[test]
    fn merge_all_staging_empty_db() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-empty";
        // Create staging DB but insert nothing
        let _s0 = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        drop(_s0);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });

        let merged =
            merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, None).unwrap();
        assert_eq!(merged, 0);
    }

    #[test]
    fn merge_all_staging_progress_callback_invoked() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);

        let ds_id = "ds-cb";
        let s0 = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        s0.execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
             VALUES ('f1', 'ds-cb', '/f1', 'f1', 'File')",
            [],
        )
        .unwrap();
        drop(s0);

        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 1,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });

        let cb_invoked = std::sync::atomic::AtomicBool::new(false);
        let cb = |completed: usize, total: usize| {
            assert_eq!(completed, 1);
            assert_eq!(total, 1);
            cb_invoked.store(true, std::sync::atomic::Ordering::Relaxed);
        };

        merge_all_staging_to_main(&main_conn, tmp.path(), ds_id, &manifest, Some(&cb)).unwrap();
        assert!(cb_invoked.load(std::sync::atomic::Ordering::Relaxed));
    }

    fn create_main_analysis_tables(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                data_source_id TEXT NOT NULL DEFAULT '',
                artifact_type TEXT NOT NULL,
                source_object_id TEXT,
                extractor_id TEXT,
                extractor_version TEXT,
                confidence REAL,
                source_attribution TEXT,
                title TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                attrs TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE timeline_events (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                source_object_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                ts TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                parser_id TEXT,
                parser_version TEXT,
                confidence REAL,
                source_attribution TEXT,
                attrs TEXT NOT NULL DEFAULT '{}'
            );",
        )
        .unwrap();
    }

    #[test]
    fn analysis_staging_open_creates_expected_tables() {
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
                    params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(name, table);
        }
    }

    #[test]
    fn analysis_staging_open_upgrades_legacy_provenance_columns() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = analysis_staging_db_path(tmp.path(), "ds-analysis", 0);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let legacy = Connection::open(&db_path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE artifact_rows (
                    id TEXT PRIMARY KEY NOT NULL,
                    file_id TEXT,
                    artifact_type TEXT NOT NULL,
                    display_name TEXT NOT NULL,
                    summary TEXT NOT NULL DEFAULT '',
                    data_json TEXT NOT NULL DEFAULT '{}',
                    source_path TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL
                );
                CREATE TABLE timeline_rows (
                    id TEXT PRIMARY KEY NOT NULL,
                    file_id TEXT NOT NULL,
                    timestamp TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    title TEXT NOT NULL,
                    description TEXT NOT NULL DEFAULT '',
                    data_json TEXT NOT NULL DEFAULT '{}'
                );
                CREATE TABLE index_docs (
                    file_id TEXT PRIMARY KEY NOT NULL,
                    path TEXT NOT NULL,
                    text TEXT NOT NULL,
                    language TEXT NOT NULL DEFAULT 'unknown',
                    truncated INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE worker_meta (
                    key TEXT PRIMARY KEY NOT NULL,
                    value TEXT NOT NULL
                );",
            )
            .unwrap();
        drop(legacy);

        let conn = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();

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
            assert!(table_has_column(&conn, table, column).unwrap());
        }
    }

    #[test]
    fn analysis_merge_skips_already_merged_worker_db() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_analysis_tables(&main_conn);
        let worker = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
        worker
            .execute(
                "INSERT INTO artifact_rows
                 (id, file_id, artifact_type, display_name, summary, data_json, source_path, created_at)
                 VALUES ('a1', 'f1', 'Prefetch', 'Artifact', '', '{}', '', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        set_worker_meta(&worker, "merged", "true").unwrap();
        drop(worker);

        let stats = merge_analysis_staging_to_main(
            &main_conn,
            tmp.path(),
            "ds-analysis",
            &[0],
            "case-1",
            &tmp.path().join("index"),
            None,
        )
        .unwrap();

        assert_eq!(stats.artifact_count, 0);
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn index_merge_uses_pages_not_full_vec() {
        let tmp = tempfile::TempDir::new().unwrap();
        let worker = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
        for i in 0..(INDEX_DOC_MERGE_PAGE_SIZE + 5) {
            worker
                .execute(
                    "INSERT INTO index_docs (file_id, path, text, language, truncated)
                     VALUES (?1, ?2, ?3, 'utf-8', 0)",
                    params![
                        format!("f-{i:03}"),
                        format!("file-{i:03}.txt"),
                        format!("marker page-test-{i:03}")
                    ],
                )
                .unwrap();
        }
        let indexed = merge_one_analysis_index_docs(&worker, &tmp.path().join("idx")).unwrap();

        assert_eq!(indexed, (INDEX_DOC_MERGE_PAGE_SIZE + 5) as u64);
    }

    #[test]
    fn analysis_merge_rolls_back_and_detaches_on_failure() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        main_conn
            .execute_batch(
                "CREATE TABLE artifacts (
                    id TEXT PRIMARY KEY NOT NULL,
                    case_id TEXT NOT NULL DEFAULT '',
                    data_source_id TEXT NOT NULL DEFAULT '',
                    artifact_type TEXT NOT NULL,
                    source_object_id TEXT,
                    extractor_id TEXT,
                    extractor_version TEXT,
                    confidence REAL,
                    source_attribution TEXT,
                    title TEXT NOT NULL,
                    summary TEXT NOT NULL DEFAULT '',
                    attrs TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );
                INSERT INTO artifacts (id, case_id, data_source_id, artifact_type, title)
                VALUES ('existing', 'case-1', 'ds-analysis', 'x', 'existing');",
            )
            .unwrap();
        let worker = open_analysis_staging(tmp.path(), "ds-analysis", 0).unwrap();
        worker
            .execute(
                "INSERT INTO artifact_rows
                 (id, file_id, artifact_type, display_name, summary, data_json, source_path, created_at)
                 VALUES ('a1', 'f1', 'Prefetch', 'Artifact', '', '{}', '', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();

        let result = merge_analysis_staging_to_main(
            &main_conn,
            tmp.path(),
            "ds-analysis",
            &[0],
            "case-1",
            &tmp.path().join("index"),
            None,
        );
        assert!(result.is_err());
        let count: i64 = main_conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(!attached_db_names(&main_conn)
            .iter()
            .any(|name| name == "analysis_stage"));
    }

    #[test]
    fn cleanup_staging_removes_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ds_id = "ds-cleanup";

        // Create staging dir + a DB, then drop connection before cleanup
        {
            let _conn = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        }
        let dir = staging_dir(tmp.path(), ds_id);
        assert!(dir.exists());

        cleanup_staging(tmp.path(), ds_id);
        assert!(!dir.exists());
    }

    #[test]
    fn staging_db_row_count_empty_returns_zero() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_partition_staging(tmp.path(), "ds-1", 0).unwrap();
        let count = staging_db_row_count(&conn).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn set_and_get_staging_meta_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_partition_staging(tmp.path(), "ds-1", 0).unwrap();

        set_staging_meta(&conn, "status", "done").unwrap();
        set_staging_meta(&conn, "file_count", "42").unwrap();

        assert_eq!(
            get_staging_meta(&conn, "status").unwrap().as_deref(),
            Some("done")
        );
        assert_eq!(
            get_staging_meta(&conn, "file_count").unwrap().as_deref(),
            Some("42")
        );
        assert_eq!(get_staging_meta(&conn, "nonexistent").unwrap(), None);
    }

    #[test]
    fn manifest_all_partitions_done_true_when_all_done() {
        let mut m = StagingManifest::create("ds-1", "/test.E01", "E01");
        m.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 10,
            dir_count: 1,
            total_size: 500,
            last_path: None,
            completed_at: None,
            error: None,
        });
        m.partitions.push(PartitionEntry {
            index: 1,
            name: "P1".to_string(),
            fs_kind: "Fat32".to_string(),
            staging_db: "partition_1.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 5,
            dir_count: 1,
            total_size: 200,
            last_path: None,
            completed_at: None,
            error: None,
        });
        assert!(m.all_partitions_done());
    }

    #[test]
    fn manifest_all_partitions_done_false_when_one_pending() {
        let mut m = StagingManifest::create("ds-1", "/test.E01", "E01");
        m.partitions.push(PartitionEntry {
            index: 0,
            name: "P0".to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 10,
            dir_count: 1,
            total_size: 500,
            last_path: None,
            completed_at: None,
            error: None,
        });
        m.partitions.push(PartitionEntry {
            index: 1,
            name: "P1".to_string(),
            fs_kind: "Fat32".to_string(),
            staging_db: "partition_1.db".to_string(),
            status: PartitionStatus::Pending,
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });
        assert!(!m.all_partitions_done());
    }

    #[test]
    fn manifest_all_partitions_done_false_when_empty() {
        let m = StagingManifest::create("ds-1", "/test.E01", "E01");
        assert!(!m.all_partitions_done());
    }

    // ------------------------------------------------------------------
    // Stage B: staging root folding into the partition placeholder
    // ------------------------------------------------------------------

    /// Insert a staging row with explicit parent/name/type for root-folding tests.
    fn insert_staging_row(
        conn: &Connection,
        ds_id: &str,
        id: &str,
        parent: Option<&str>,
        name: &str,
        entry_type: &str,
    ) {
        conn.execute(
            "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, parent, ds_id, format!("/{name}"), name, entry_type],
        )
        .unwrap();
    }

    fn single_done_manifest(ds_id: &str, name: &str) -> StagingManifest {
        let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
        manifest.partitions.push(PartitionEntry {
            index: 0,
            name: name.to_string(),
            fs_kind: "Ntfs".to_string(),
            staging_db: "partition_0.db".to_string(),
            status: PartitionStatus::Done,
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });
        manifest
    }

    /// Pre-seed a partition-0 placeholder root in the main DB and return its id.
    fn seed_placeholder(main_conn: &Connection, ds_id: &str, name: &str) -> String {
        crate::file_service::insert_partition_placeholder_root(
            main_conn,
            &domain::DataSourceId(ds_id.to_string()),
            0,
            name,
            "queued",
        )
        .unwrap()
        .0
    }

    fn first_level_roots(main_conn: &Connection) -> Vec<String> {
        let mut stmt = main_conn
            .prepare("SELECT name FROM file_entries WHERE parent_id IS NULL ORDER BY name")
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn placeholder_index_lookup_does_not_collide_on_digit_prefix() {
        // Regression guard: the index lookup GLOB is `__partition_placeholder__/{index}/*`.
        // A query for index 1 must NOT match index 12's placeholder, because the
        // literal `/` after the index anchors the segment. Seed ONLY index 12,
        // then look up index 1 — it must return None, never partition 12's root.
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);
        let ds_id = "ds-glob-collision";

        let id_12 = crate::file_service::insert_partition_placeholder_root(
            &main_conn,
            &domain::DataSourceId(ds_id.to_string()),
            12,
            "Partition 12 (NTFS)",
            "queued",
        )
        .unwrap()
        .0;

        // Looking up index 1 must not find index 12's placeholder.
        let one = find_partition_placeholder_root_id_by_index(&main_conn, ds_id, 1).unwrap();
        assert_eq!(
            one, None,
            "index 1 lookup must not match index 12 placeholder"
        );

        // And index 12 finds exactly its own placeholder.
        let twelve = find_partition_placeholder_root_id_by_index(&main_conn, ds_id, 12).unwrap();
        assert_eq!(twelve.as_deref(), Some(id_12.as_str()));

        // Conversely, seed index 1 too and confirm each resolves to itself.
        let id_1 = crate::file_service::insert_partition_placeholder_root(
            &main_conn,
            &domain::DataSourceId(ds_id.to_string()),
            1,
            "Partition 1 (NTFS)",
            "queued",
        )
        .unwrap()
        .0;
        assert_eq!(
            find_partition_placeholder_root_id_by_index(&main_conn, ds_id, 1)
                .unwrap()
                .as_deref(),
            Some(id_1.as_str())
        );
        assert_eq!(
            find_partition_placeholder_root_id_by_index(&main_conn, ds_id, 12)
                .unwrap()
                .as_deref(),
            Some(id_12.as_str())
        );
    }

    #[test]
    fn merge_folds_null_parent_synthetic_root_and_reparents_children() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);
        let ds_id = "ds-fold-null-root";
        let ph = seed_placeholder(&main_conn, ds_id, "Partition 0 (NTFS)");

        let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        // Synthetic NTFS root (NULL parent, name `\`) + a child directory.
        insert_staging_row(&staging, ds_id, "root5", None, "\\", "directory");
        insert_staging_row(
            &staging,
            ds_id,
            "win",
            Some("root5"),
            "Windows",
            "directory",
        );
        drop(staging);

        merge_all_staging_to_main(
            &main_conn,
            tmp.path(),
            ds_id,
            &single_done_manifest(ds_id, "Partition 0 (NTFS)"),
            None,
        )
        .unwrap();

        // Synthetic `\` root is not inserted; the only first-level root is the partition.
        assert_eq!(
            first_level_roots(&main_conn),
            vec!["Partition 0 (NTFS)".to_string()]
        );
        // Child re-parented onto the placeholder.
        let win_parent: String = main_conn
            .query_row(
                "SELECT parent_id FROM file_entries WHERE id = 'win'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(win_parent, ph);
    }

    #[test]
    fn merge_folds_self_referential_root() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);
        let ds_id = "ds-fold-self-root";
        let ph = seed_placeholder(&main_conn, ds_id, "Partition 0 (NTFS)");

        let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        // Self-referential root (parent_id = id, name `.`).
        insert_staging_row(
            &staging,
            ds_id,
            "selfroot",
            Some("selfroot"),
            ".",
            "directory",
        );
        insert_staging_row(
            &staging,
            ds_id,
            "docs",
            Some("selfroot"),
            "Docs",
            "directory",
        );
        drop(staging);

        merge_all_staging_to_main(
            &main_conn,
            tmp.path(),
            ds_id,
            &single_done_manifest(ds_id, "Partition 0 (NTFS)"),
            None,
        )
        .unwrap();

        assert_eq!(
            first_level_roots(&main_conn),
            vec!["Partition 0 (NTFS)".to_string()]
        );
        let docs_parent: String = main_conn
            .query_row(
                "SELECT parent_id FROM file_entries WHERE id = 'docs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(docs_parent, ph);
    }

    #[test]
    fn merge_folds_slash_named_root() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);
        let ds_id = "ds-fold-slash-root";
        seed_placeholder(&main_conn, ds_id, "Partition 0 (FAT)");

        let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        insert_staging_row(&staging, ds_id, "root", None, "/", "directory");
        insert_staging_row(&staging, ds_id, "f1", Some("root"), "boot.ini", "file");
        drop(staging);

        merge_all_staging_to_main(
            &main_conn,
            tmp.path(),
            ds_id,
            &single_done_manifest(ds_id, "Partition 0 (FAT)"),
            None,
        )
        .unwrap();

        assert_eq!(
            first_level_roots(&main_conn),
            vec!["Partition 0 (FAT)".to_string()]
        );
    }

    #[test]
    fn merge_synthesizes_root_when_placeholder_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);
        let ds_id = "ds-synth-missing";
        // NOTE: no placeholder seeded — exercises the synthesis branch.

        let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        insert_staging_row(&staging, ds_id, "root5", None, "\\", "directory");
        insert_staging_row(
            &staging,
            ds_id,
            "win",
            Some("root5"),
            "Windows",
            "directory",
        );
        drop(staging);

        merge_all_staging_to_main(
            &main_conn,
            tmp.path(),
            ds_id,
            &single_done_manifest(ds_id, "Partition 0 (NTFS)"),
            None,
        )
        .unwrap();

        // No bare `\` leaks to the first level — a partition root is synthesized.
        let roots = first_level_roots(&main_conn);
        assert_eq!(roots, vec!["Partition 0 (NTFS)".to_string()]);
        assert!(!roots.iter().any(|n| n == "\\"));
        // The child hangs under the synthesized+promoted root.
        let win_parent: Option<String> = main_conn
            .query_row(
                "SELECT parent_id FROM file_entries WHERE id = 'win'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let root_id: String = main_conn
            .query_row(
                "SELECT id FROM file_entries WHERE parent_id IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(win_parent, Some(root_id));
    }

    #[test]
    fn merge_keeps_fat_top_level_entries_under_partition_root() {
        let tmp = tempfile::TempDir::new().unwrap();
        let main_conn = persistence_sqlite::connection::open_in_memory().unwrap();
        create_main_file_entries_table(&main_conn);
        let ds_id = "ds-fat-efi";
        let ph = seed_placeholder(&main_conn, ds_id, "Partition 0 (FAT)");

        // FAT path has no synthetic root: real top-level entries (EFI) have a
        // NULL parent directly and must be re-parented (kept), not dropped.
        let staging = open_partition_staging(tmp.path(), ds_id, 0).unwrap();
        insert_staging_row(&staging, ds_id, "efi", None, "EFI", "directory");
        insert_staging_row(&staging, ds_id, "boot", Some("efi"), "Boot", "directory");
        drop(staging);

        merge_all_staging_to_main(
            &main_conn,
            tmp.path(),
            ds_id,
            &single_done_manifest(ds_id, "Partition 0 (FAT)"),
            None,
        )
        .unwrap();

        // First level is only the partition root; EFI is now its child.
        assert_eq!(
            first_level_roots(&main_conn),
            vec!["Partition 0 (FAT)".to_string()]
        );
        let efi_parent: String = main_conn
            .query_row(
                "SELECT parent_id FROM file_entries WHERE id = 'efi'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(efi_parent, ph);
        // EFI itself is retained (not dropped as a synthetic root).
        let efi_exists: i64 = main_conn
            .query_row(
                "SELECT COUNT(*) FROM file_entries WHERE id = 'efi'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(efi_exists, 1);
    }
}
