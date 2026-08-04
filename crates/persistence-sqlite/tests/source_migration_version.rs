use persistence_sqlite::runner;

#[test]
fn source_version_order_accepts_equal_and_newer_versions() {
    assert!(runner::source_version_is_at_least(
        "source_015_ceph_bluestore_rbd_header_context",
        "source_015_ceph_bluestore_rbd_header_context"
    ));
    assert!(runner::source_version_is_at_least(
        "source_016_file_partition_index",
        "source_015_ceph_bluestore_rbd_header_context"
    ));
    assert!(runner::source_version_is_at_least(
        "source_017_timeline_projection_identity",
        "source_016_file_partition_index"
    ));
    assert!(runner::source_version_is_at_least(
        "source_018_cephfs_metadata_inventory",
        "source_017_timeline_projection_identity"
    ));
    assert!(runner::source_version_is_at_least(
        "source_019_cephfs_journal_replay",
        "source_018_cephfs_metadata_inventory"
    ));
    assert!(runner::source_version_is_at_least(
        "source_020_cephfs_namespace_layout",
        "source_019_cephfs_journal_replay"
    ));
    assert!(runner::source_version_is_at_least(
        "source_021_cephfs_assembly_capability",
        "source_020_cephfs_namespace_layout"
    ));
    assert!(runner::source_version_is_at_least(
        "source_022_file_partition_index_repair",
        "source_021_cephfs_assembly_capability"
    ));
    assert!(runner::source_version_is_at_least(
        "source_024_ntfs_deleted_recovery",
        "source_022_file_partition_index_repair"
    ));
    assert!(runner::source_version_is_at_least(
        "source_025_file_entry_encrypted",
        "source_024_ntfs_deleted_recovery"
    ));
    assert!(runner::source_version_is_at_least(
        "source_026_timeline_keyset_indexes",
        "source_025_file_entry_encrypted"
    ));
    assert!(runner::source_version_is_at_least(
        "source_028_file_entry_read_only",
        "source_026_timeline_keyset_indexes"
    ));
    assert!(runner::source_version_is_at_least(
        "source_030_analysis_file_feed_index",
        "source_028_file_entry_read_only"
    ));
    assert!(runner::source_version_is_at_least(
        "source_031_mount_directory_index",
        "source_030_analysis_file_feed_index"
    ));
}

#[test]
fn source_version_order_rejects_older_and_unknown_versions() {
    assert!(!runner::source_version_is_at_least(
        "source_014_ceph_osd_device_bindings",
        "source_015_ceph_bluestore_rbd_header_context"
    ));
    assert!(!runner::source_version_is_at_least(
        "source_999_unknown",
        "source_015_ceph_bluestore_rbd_header_context"
    ));
    assert!(!runner::source_version_is_at_least(
        "source_017_timeline_projection_identity",
        "source_999_unknown"
    ));
}

#[test]
fn source_024_through_031_upgrade_preserves_rows_and_adds_query_indexes() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
    connection
        .execute_batch(include_str!("../src/migrations/scripts/source_001.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!(
            "../src/migrations/scripts/source_023_deleted_recovery.sql"
        ))
        .unwrap();
    connection
        .execute_batch(
            "INSERT INTO schema_migrations (name) VALUES
                ('source_001'),
                ('source_002_data_source_metadata'),
                ('source_003_correlation_cache'),
                ('source_004_graph_node_order_index'),
                ('source_005_ceph_osd_inventory'),
                ('source_006_ceph_bluefs_inventory'),
                ('source_007_ceph_bluefs_replay'),
                ('source_008_ceph_rocksdb_inventory'),
                ('source_009_ceph_sst_inventory'),
                ('source_010_ceph_wal_inventory'),
                ('source_011_ceph_latest_state'),
                ('source_012_ceph_bluestore_semantics'),
                ('source_013_ceph_bluestore_omap'),
                ('source_014_ceph_osd_device_bindings'),
                ('source_015_ceph_bluestore_rbd_header_context'),
                ('source_016_file_partition_index'),
                ('source_017_timeline_projection_identity'),
                ('source_018_cephfs_metadata_inventory'),
                ('source_019_cephfs_journal_replay'),
                ('source_020_cephfs_namespace_layout'),
                ('source_021_cephfs_assembly_capability'),
                ('source_022_file_partition_index_repair'),
                ('source_023_deleted_recovery')",
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
             VALUES ('source-migration', 'case-1', 'Migration fixture', 'e01', 'fixture.E01', '2026-07-21T00:00:00Z')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type)
             VALUES ('legacy-file', 'source-migration', 'legacy.bin', 'legacy.bin', 'file')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO filesystem_recovery_scans (
                id, data_source_id, partition_index, filesystem_type,
                parser_version, log_kind, snapshot_identity_sha256,
                state, candidate_count, started_at, completed_at
             ) VALUES (
                'scan-old', 'source-migration', 2, 'xfs', 'xfs-log-v1',
                'internal_log', ?1, 'partial', 1,
                '2026-07-21T00:00:00Z', '2026-07-21T00:00:01Z'
             )",
            ["a".repeat(64)],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO deleted_file_recoveries (
                id, scan_id, inode, entry_type, declared_size,
                completeness, recovery_method, confidence, allocation_state
             ) VALUES (
                'recovery-old', 'scan-old', '77', 'file', 0,
                'metadata_only', 'xfs-log-inode', 0.5, 'unverified'
             )",
            [],
        )
        .unwrap();

    assert_eq!(runner::run_source_all(&connection).unwrap(), 8);

    let encrypted_column: (String, i64, Option<String>) = connection
        .query_row(
            "SELECT type, \"notnull\", dflt_value
             FROM pragma_table_info('file_entries')
             WHERE name = 'encrypted'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("source_025 must add the encrypted column");
    assert_eq!(encrypted_column, ("INTEGER".to_string(), 0, None));
    let legacy_encryption_status: Option<i64> = connection
        .query_row(
            "SELECT encrypted FROM file_entries WHERE id = 'legacy-file'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(legacy_encryption_status, None);

    let old_sequence: Option<u16> = connection
        .query_row(
            "SELECT mft_sequence FROM deleted_file_recoveries WHERE id = 'recovery-old'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old_sequence, None);

    connection
        .execute(
            "INSERT INTO filesystem_recovery_scans (
                id, data_source_id, partition_index, filesystem_type,
                parser_version, log_kind, snapshot_identity_sha256,
                state, candidate_count, started_at, completed_at
             ) VALUES (
                'scan-ntfs', 'source-migration', 3, 'ntfs', 'ntfs-mft-v1',
                'internal_log', ?1, 'complete', 1,
                '2026-07-21T00:00:00Z', '2026-07-21T00:00:02Z'
             )",
            ["b".repeat(64)],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO deleted_file_recoveries (
                id, scan_id, inode, entry_type, mft_sequence, declared_size,
                completeness, recovery_method, confidence, allocation_state
             ) VALUES (
                'recovery-ntfs', 'scan-ntfs', '88', 'file', 17, 0,
                'complete', 'ntfs-mft-v1', 0.92, 'free'
             )",
            [],
        )
        .unwrap();
    let sequence: u16 = connection
        .query_row(
            "SELECT mft_sequence FROM deleted_file_recoveries WHERE id = 'recovery-ntfs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(sequence, 17);

    let keyset_index_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index'
               AND name IN (
                   'idx_source_timeline_ts_id',
                   'idx_source_timeline_type_ts_id',
                   'idx_source_artifacts_created_id',
                   'idx_source_artifacts_type_created_id'
               )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(keyset_index_count, 4);
    let entity_projection_index: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_source_graph_nodes_case_type_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(entity_projection_index, 1);

    let analysis_feed_index_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_source_file_entries_analysis_feed'",
            [],
            |row| row.get(0),
        )
        .expect("source_030 must add the analysis keyset feed index");
    assert!(analysis_feed_index_sql.contains("data_source_id, path ASC, id ASC"));
    assert!(analysis_feed_index_sql.contains("WHERE LOWER(entry_type) = 'file'"));

    let mount_index_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_source_file_entries_mount_children'",
            [],
            |row| row.get(0),
        )
        .expect("source_031 must add the mount directory index");
    assert!(mount_index_sql.contains("parent_id"));
    assert!(mount_index_sql.contains("name COLLATE NOCASE"));
    assert!(!mount_index_sql.contains("WHERE deleted = 0"));
}

#[test]
fn current_source_schema_repairs_a_missing_analysis_feed_index() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    assert!(runner::run_source_all(&connection).unwrap() > 0);
    connection
        .execute_batch("DROP INDEX idx_source_file_entries_analysis_feed")
        .unwrap();

    assert_eq!(runner::run_source_all(&connection).unwrap(), 0);
    let repaired: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_source_file_entries_analysis_feed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(repaired.contains("WHERE LOWER(entry_type) = 'file'"));
}

#[test]
fn current_source_schema_repairs_an_outdated_mount_directory_index() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    assert!(runner::run_source_all(&connection).unwrap() > 0);
    connection
        .execute_batch(
            "DROP INDEX idx_source_file_entries_mount_children;
             CREATE INDEX idx_source_file_entries_mount_children
             ON file_entries(parent_id, data_source_id, partition_index);",
        )
        .unwrap();

    assert_eq!(runner::run_source_all(&connection).unwrap(), 0);
    let repaired: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_source_file_entries_mount_children'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(repaired.contains("name COLLATE NOCASE"));
    assert!(!repaired.contains("WHERE deleted = 0"));
}

#[test]
fn current_source_schema_rejects_a_missing_file_catalog() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    assert!(runner::run_source_all(&connection).unwrap() > 0);
    connection.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
    connection.execute_batch("DROP TABLE file_entries").unwrap();

    let error = runner::run_source_all(&connection).unwrap_err();
    assert!(error
        .to_string()
        .contains("source_030 requires file_entries"));
}
