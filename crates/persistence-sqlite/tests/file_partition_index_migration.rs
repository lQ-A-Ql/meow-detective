use persistence_sqlite::runner;
use rusqlite::{params, Connection};

const PRIOR_SOURCE_MIGRATIONS: &[&str] = &[
    "source_001",
    "source_002_data_source_metadata",
    "source_003_correlation_cache",
    "source_004_graph_node_order_index",
    "source_005_ceph_osd_inventory",
    "source_006_ceph_bluefs_inventory",
    "source_007_ceph_bluefs_replay",
    "source_008_ceph_rocksdb_inventory",
    "source_009_ceph_sst_inventory",
    "source_010_ceph_wal_inventory",
    "source_011_ceph_latest_state",
    "source_012_ceph_bluestore_semantics",
    "source_013_ceph_bluestore_omap",
    "source_014_ceph_osd_device_bindings",
    "source_015_ceph_bluestore_rbd_header_context",
];

fn legacy_source_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=OFF;
         CREATE TABLE schema_migrations (
             id INTEGER PRIMARY KEY,
             name TEXT NOT NULL UNIQUE,
             applied_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE TABLE file_entries (
             id TEXT PRIMARY KEY NOT NULL,
             parent_id TEXT REFERENCES file_entries(id),
             data_source_id TEXT NOT NULL,
             path TEXT NOT NULL,
             name TEXT NOT NULL,
             entry_type TEXT NOT NULL,
             partition_index INTEGER
         );
         CREATE TABLE artifacts (
             id TEXT PRIMARY KEY NOT NULL,
             artifact_type TEXT NOT NULL DEFAULT '',
             source_object_id TEXT,
             extractor_id TEXT,
             created_at TEXT NOT NULL DEFAULT ''
         );
         CREATE TABLE timeline_events (
             id TEXT PRIMARY KEY NOT NULL,
             event_type TEXT NOT NULL,
             ts TEXT NOT NULL,
             source_object_id TEXT NOT NULL,
             parser_id TEXT
         );
         CREATE TABLE data_sources (
             id TEXT PRIMARY KEY NOT NULL
         );
         CREATE TABLE ceph_bluestore_semantic_scans (
             inventory_id TEXT PRIMARY KEY NOT NULL
         );
         CREATE TABLE ceph_bluestore_objects (
             inventory_id TEXT NOT NULL,
             object_identity_sha256 TEXT NOT NULL,
             decoded_pool INTEGER NOT NULL,
             PRIMARY KEY (inventory_id, object_identity_sha256)
         );",
    )
    .unwrap();
    for migration in PRIOR_SOURCE_MIGRATIONS {
        conn.execute(
            "INSERT INTO schema_migrations(name) VALUES (?1)",
            params![migration],
        )
        .unwrap();
    }
    conn
}

fn insert_entry(
    conn: &Connection,
    id: &str,
    parent_id: Option<&str>,
    data_source_id: &str,
    name: &str,
    partition_index: Option<i64>,
) {
    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, partition_index)
         VALUES (?1, ?2, ?3, '', ?4, 'directory', ?5)",
        params![id, parent_id, data_source_id, name, partition_index],
    )
    .unwrap();
}

fn source_021_connection_with_late_file_rows() -> Connection {
    let conn = legacy_source_connection();
    for migration in [
        "source_016_file_partition_index",
        "source_017_timeline_projection_identity",
        "source_018_cephfs_metadata_inventory",
        "source_019_cephfs_journal_replay",
        "source_020_cephfs_namespace_layout",
        "source_021_cephfs_assembly_capability",
    ] {
        conn.execute(
            "INSERT INTO schema_migrations(name) VALUES (?1)",
            params![migration],
        )
        .unwrap();
    }
    insert_entry(
        &conn,
        "late-root",
        None,
        "ds-late",
        "Partition 2 (XFS) - cl/root",
        None,
    );
    insert_entry(
        &conn,
        "late-child",
        Some("late-root"),
        "ds-late",
        "usr",
        None,
    );
    conn
}

#[test]
fn source_016_backfills_only_reliable_partition_roots_and_descendants() {
    let conn = legacy_source_connection();
    insert_entry(&conn, "root-2", None, "ds-a", "Partition 2 (XFS)", None);
    insert_entry(&conn, "etc", Some("root-2"), "ds-a", "etc", None);
    insert_entry(&conn, "ssh", Some("etc"), "ds-a", "ssh", None);
    insert_entry(&conn, "bad", None, "ds-a", "Partition 7broken", None);
    insert_entry(&conn, "bad-child", Some("bad"), "ds-a", "child", None);
    insert_entry(&conn, "unknown", None, "ds-a", "Volume (XFS)", None);
    insert_entry(&conn, "preset", None, "ds-a", "Partition 3", Some(99));

    assert_eq!(runner::run_source_all(&conn).unwrap(), 12);
    assert_eq!(
        runner::latest_source_version(),
        "source_027_artifact_keyset_indexes"
    );

    for id in ["root-2", "etc", "ssh"] {
        let partition_index: Option<i64> = conn
            .query_row(
                "SELECT partition_index FROM file_entries WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(partition_index, Some(2), "{id}");
    }
    for id in ["bad", "bad-child", "unknown"] {
        let partition_index: Option<i64> = conn
            .query_row(
                "SELECT partition_index FROM file_entries WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(partition_index, None, "{id}");
    }
    let preset: i64 = conn
        .query_row(
            "SELECT partition_index FROM file_entries WHERE id = 'preset'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(preset, 99);
    let trigger_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'trigger' AND tbl_name = 'file_entries'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(trigger_count, 0);
    let analysis_index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index'
               AND name IN (
                   'idx_source_artifacts_analysis_output',
                   'idx_source_timeline_analysis_output'
               )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(analysis_index_count, 2);
}

#[test]
fn source_016_adds_partition_column_for_staging_compatible_catalogs() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
             id INTEGER PRIMARY KEY,
             name TEXT NOT NULL UNIQUE,
             applied_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE TABLE file_entries (
             id TEXT PRIMARY KEY NOT NULL,
             parent_id TEXT REFERENCES file_entries(id),
             data_source_id TEXT NOT NULL,
             path TEXT NOT NULL,
             name TEXT NOT NULL,
             entry_type TEXT NOT NULL
         );
         CREATE TABLE artifacts (
             id TEXT PRIMARY KEY NOT NULL,
             artifact_type TEXT NOT NULL DEFAULT '',
             source_object_id TEXT,
             extractor_id TEXT,
             created_at TEXT NOT NULL DEFAULT ''
         );
         CREATE TABLE timeline_events (
             id TEXT PRIMARY KEY NOT NULL,
             event_type TEXT NOT NULL,
             ts TEXT NOT NULL,
             source_object_id TEXT NOT NULL,
             parser_id TEXT
         );
         CREATE TABLE data_sources (
             id TEXT PRIMARY KEY NOT NULL
         );
         CREATE TABLE ceph_bluestore_semantic_scans (
             inventory_id TEXT PRIMARY KEY NOT NULL
         );
         CREATE TABLE ceph_bluestore_objects (
             inventory_id TEXT NOT NULL,
             object_identity_sha256 TEXT NOT NULL,
             decoded_pool INTEGER NOT NULL,
             PRIMARY KEY (inventory_id, object_identity_sha256)
         );
         INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type)
         VALUES
             ('root-4', NULL, 'ds-a', '', 'Partition 4 (XFS)', 'directory'),
             ('child', 'root-4', 'ds-a', 'etc', 'etc', 'directory');",
    )
    .unwrap();
    for migration in PRIOR_SOURCE_MIGRATIONS {
        conn.execute(
            "INSERT INTO schema_migrations(name) VALUES (?1)",
            params![migration],
        )
        .unwrap();
    }

    assert_eq!(runner::run_source_all(&conn).unwrap(), 12);
    for id in ["root-4", "child"] {
        let partition_index: Option<i64> = conn
            .query_row(
                "SELECT partition_index FROM file_entries WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(partition_index, Some(4), "{id}");
    }
}

#[test]
fn source_022_repairs_rows_added_after_source_016() {
    let conn = source_021_connection_with_late_file_rows();

    assert_eq!(runner::run_source_all(&conn).unwrap(), 6);
    for id in ["late-root", "late-child"] {
        let partition_index: Option<i64> = conn
            .query_row(
                "SELECT partition_index FROM file_entries WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(partition_index, Some(2), "{id}");
    }
}

#[test]
fn source_022_repairs_a_catalog_created_after_source_016_was_skipped() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
             id INTEGER PRIMARY KEY,
             name TEXT NOT NULL UNIQUE,
             applied_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE TABLE file_entries (
             id TEXT PRIMARY KEY NOT NULL,
             parent_id TEXT REFERENCES file_entries(id),
             data_source_id TEXT NOT NULL,
             path TEXT NOT NULL,
             name TEXT NOT NULL,
             entry_type TEXT NOT NULL
         );
         CREATE TABLE timeline_events (
             id TEXT PRIMARY KEY NOT NULL,
             event_type TEXT NOT NULL,
             ts TEXT NOT NULL
         );
         CREATE TABLE artifacts (
             id TEXT PRIMARY KEY NOT NULL,
             artifact_type TEXT NOT NULL DEFAULT '',
             created_at TEXT NOT NULL DEFAULT ''
         );
         INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type)
         VALUES
             ('late-root-no-column', NULL, 'ds-late', '', 'Partition 3 (XFS)', 'directory');",
    )
    .unwrap();
    for migration in [
        "source_001",
        "source_002_data_source_metadata",
        "source_003_correlation_cache",
        "source_004_graph_node_order_index",
        "source_005_ceph_osd_inventory",
        "source_006_ceph_bluefs_inventory",
        "source_007_ceph_bluefs_replay",
        "source_008_ceph_rocksdb_inventory",
        "source_009_ceph_sst_inventory",
        "source_010_ceph_wal_inventory",
        "source_011_ceph_latest_state",
        "source_012_ceph_bluestore_semantics",
        "source_013_ceph_bluestore_omap",
        "source_014_ceph_osd_device_bindings",
        "source_015_ceph_bluestore_rbd_header_context",
        "source_016_file_partition_index",
        "source_017_timeline_projection_identity",
        "source_018_cephfs_metadata_inventory",
        "source_019_cephfs_journal_replay",
        "source_020_cephfs_namespace_layout",
        "source_021_cephfs_assembly_capability",
    ] {
        conn.execute(
            "INSERT INTO schema_migrations(name) VALUES (?1)",
            params![migration],
        )
        .unwrap();
    }

    assert_eq!(runner::run_source_all(&conn).unwrap(), 6);
    let partition_index: Option<i64> = conn
        .query_row(
            "SELECT partition_index FROM file_entries WHERE id = 'late-root-no-column'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(partition_index, Some(3));
}
