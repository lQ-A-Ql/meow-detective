use persistence_sqlite::{open_in_memory, runner};
use rusqlite::Connection;

#[test]
fn application_and_source_schemas_carry_nullable_unix_mode() {
    let application = open_in_memory().expect("open application database");
    runner::run_all(&application).expect("run application migrations");
    seed_application_source(&application);
    assert_unix_mode_contract(&application, "app-file", "app-source");

    let source = open_in_memory().expect("open source database");
    runner::run_source_all(&source).expect("run source migrations");
    seed_source_registration(&source);
    assert_unix_mode_contract(&source, "source-file", "source-1");
}

#[test]
fn upgrade_from_source_034_adds_unix_mode_without_losing_rows() {
    let connection = open_in_memory().expect("open legacy source database");
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .expect("create migration ledger");
    connection
        .execute_batch(include_str!("../src/migrations/scripts/source_001.sql"))
        .expect("create legacy source schema");
    for name in PRIOR_SOURCE_MIGRATIONS {
        connection
            .execute("INSERT INTO schema_migrations (name) VALUES (?1)", [name])
            .expect("record prior migration");
    }
    connection
        .execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
             VALUES ('source-legacy', 'case-1', 'Legacy', 'e01', 'fixture.E01', '2026-08-01T00:00:00Z')",
            [],
        )
        .expect("insert legacy data source");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type)
             VALUES ('legacy-file', 'source-legacy', 'legacy.bin', 'legacy.bin', 'file')",
            [],
        )
        .expect("insert legacy file row");

    assert_eq!(runner::run_source_all(&connection).expect("upgrade"), 1);

    let row: (String, Option<i64>) = connection
        .query_row(
            "SELECT path, unix_mode FROM file_entries WHERE id = 'legacy-file'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read upgraded row");
    assert_eq!(row, ("legacy.bin".to_string(), None));
}

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
    "source_016_file_partition_index",
    "source_017_timeline_projection_identity",
    "source_018_cephfs_metadata_inventory",
    "source_019_cephfs_journal_replay",
    "source_020_cephfs_namespace_layout",
    "source_021_cephfs_assembly_capability",
    "source_022_file_partition_index_repair",
    "source_023_deleted_recovery",
    "source_024_ntfs_deleted_recovery",
    "source_025_file_entry_encrypted",
    "source_026_timeline_keyset_indexes",
    "source_027_artifact_keyset_indexes",
    "source_028_file_entry_read_only",
    "source_029_case_graph_entity_index",
    "source_030_analysis_file_feed_index",
    "source_031_mount_directory_index",
    "source_032_deleted_recovery_hashes",
    "source_033_timeline_case_id_index",
    "source_034_file_entry_archive",
];

fn assert_unix_mode_contract(conn: &Connection, file_id: &str, data_source_id: &str) {
    let column: (String, i64, Option<String>) = conn
        .query_row(
            "SELECT type, \"notnull\", dflt_value
             FROM pragma_table_info('file_entries')
             WHERE name = 'unix_mode'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("unix_mode column");
    assert_eq!(column, ("INTEGER".to_string(), 0, None));

    conn.execute(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type)
         VALUES (?1, ?2, 'plain.txt', 'plain.txt', 'file')",
        (file_id, data_source_id),
    )
    .expect("insert file without unix mode");
    let default_value: Option<i64> = conn
        .query_row(
            "SELECT unix_mode FROM file_entries WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .expect("read default state");
    assert_eq!(default_value, None);

    conn.execute(
        "UPDATE file_entries SET unix_mode = 33188 WHERE id = ?1",
        [file_id],
    )
    .expect("persist unix mode");
    let stored: Option<i64> = conn
        .query_row(
            "SELECT unix_mode FROM file_entries WHERE id = ?1",
            [file_id],
            |row| row.get(0),
        )
        .expect("read persisted unix mode");
    assert_eq!(stored, Some(0o100644i64));
}

fn seed_application_source(conn: &Connection) {
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-1', 'case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("insert application case");
    seed_source_registration(conn);
}

fn seed_source_registration(conn: &Connection) {
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES (?1, 'case-1', 'source', 'e01', '', '2026-01-01T00:00:00Z')",
        [if table_has_case(conn) {
            "app-source"
        } else {
            "source-1"
        }],
    )
    .expect("insert data source");
}

fn table_has_case(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM cases WHERE id = 'case-1')",
        [],
        |row| row.get(0),
    )
    .unwrap_or(false)
}
