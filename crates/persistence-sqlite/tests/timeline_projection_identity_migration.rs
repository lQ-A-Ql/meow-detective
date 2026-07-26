use persistence_sqlite::runner;
use rusqlite::{params, Connection};

const SOURCE_016_MIGRATIONS: &[&str] = &[
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
];

#[test]
fn fresh_source_schema_contains_timeline_projection_identity() {
    let connection = Connection::open_in_memory().expect("open source database");

    runner::run_source_all(&connection).expect("run source migrations");

    let input_identity_default: String = connection
        .query_row(
            "SELECT dflt_value
             FROM pragma_table_info('timeline_projection_meta')
             WHERE name = 'input_identity'",
            [],
            |row| row.get(0),
        )
        .expect("read input identity column");
    assert_eq!(input_identity_default, "''");
    assert_eq!(
        runner::current_version(&connection)
            .expect("read source version")
            .as_deref(),
        Some("source_027_artifact_keyset_indexes")
    );
}

#[test]
fn source_016_projection_metadata_is_upgraded_without_losing_rows() {
    let connection = Connection::open_in_memory().expect("open source database");
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL UNIQUE,
                 applied_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE timeline_projection_meta (
                 projection_key TEXT PRIMARY KEY NOT NULL,
                 status TEXT NOT NULL,
                 inserted_count INTEGER NOT NULL DEFAULT 0,
                 updated_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE data_sources (
                 id TEXT PRIMARY KEY NOT NULL
             );
             CREATE TABLE file_entries (
                 id TEXT PRIMARY KEY NOT NULL,
                 parent_id TEXT,
                 data_source_id TEXT NOT NULL,
                 name TEXT NOT NULL,
                 partition_index INTEGER
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
             CREATE TABLE ceph_bluestore_semantic_scans (
                 inventory_id TEXT PRIMARY KEY NOT NULL
             );
             CREATE TABLE ceph_bluestore_objects (
                 inventory_id TEXT NOT NULL,
                 object_identity_sha256 TEXT NOT NULL,
                 decoded_pool INTEGER NOT NULL,
                 PRIMARY KEY (inventory_id, object_identity_sha256)
             );
             INSERT INTO timeline_projection_meta
                 (projection_key, status, inserted_count)
             VALUES ('macb', 'done', 42);",
        )
        .expect("create source 016 projection metadata");
    for migration in SOURCE_016_MIGRATIONS {
        connection
            .execute(
                "INSERT INTO schema_migrations(name) VALUES (?1)",
                params![migration],
            )
            .expect("record prior source migration");
    }

    assert_eq!(
        runner::run_source_all(&connection).expect("upgrade source database"),
        11
    );
    let row: (String, i64, String) = connection
        .query_row(
            "SELECT status, inserted_count, input_identity
             FROM timeline_projection_meta
             WHERE projection_key = 'macb'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read upgraded projection metadata");
    assert_eq!(row, ("done".to_string(), 42, String::new()));
}
