use super::*;

#[test]
fn migrate_partitions_updates_0014_migration_log() {
    let conn = crate::connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE data_sources (
            id TEXT PRIMARY KEY,
            case_id TEXT,
            name TEXT,
            kind TEXT,
            source_path TEXT,
            imported_at TEXT,
            partitions TEXT
        );
        CREATE TABLE data_source_partitions (
            id TEXT PRIMARY KEY,
            data_source_id TEXT REFERENCES data_sources(id),
            partition_index INTEGER,
            name TEXT,
            kind_label TEXT,
            status TEXT,
            type_guid TEXT,
            offset INTEGER,
            length INTEGER,
            filesystem TEXT,
            unlock_hint TEXT,
            lvm_vg_uuid TEXT,
            lvm_vg_name TEXT,
            lvm_lv_uuid TEXT,
            lvm_lv_name TEXT,
            lvm_pv_offsets_json TEXT,
            lvm_pv_sources_json TEXT
        );
        CREATE TABLE migration_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            migration_name TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            details TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        INSERT INTO migration_log (migration_name, status, details)
            VALUES ('0014_migrate_partitions', 'pending', 'Waiting for application-layer migration');",
    )
    .unwrap();

    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at, partitions)
         VALUES ('ds1', 'c1', 'Test', 'E01', '/path', '2024-01-01', ?1)",
        [r#"[{"name":"Partition 1","kind_label":"NTFS","status":"supported","offset":0,"length":1048576,"filesystem":"NTFS"}]"#],
    )
    .unwrap();

    let result = migrate_partitions(&conn).unwrap();
    assert_eq!(result.migrated_count, 1);

    let (status, details): (String, String) = conn
        .query_row(
            "SELECT status, details FROM migration_log WHERE migration_name = '0014_migrate_partitions'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "completed");
    assert!(details.contains("Migrated: 1"));

    let stale_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM migration_log WHERE migration_name = '0012_migrate_partitions'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stale_count, 0);
}

#[test]
fn migrate_partitions_succeeds_without_migration_log_table() {
    let conn = crate::connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE data_sources (
            id TEXT PRIMARY KEY,
            case_id TEXT,
            name TEXT,
            kind TEXT,
            source_path TEXT,
            imported_at TEXT,
            partitions TEXT
        );
        CREATE TABLE data_source_partitions (
            id TEXT PRIMARY KEY,
            data_source_id TEXT REFERENCES data_sources(id),
            partition_index INTEGER,
            name TEXT,
            kind_label TEXT,
            status TEXT,
            type_guid TEXT,
            offset INTEGER,
            length INTEGER,
            filesystem TEXT,
            unlock_hint TEXT,
            lvm_vg_uuid TEXT,
            lvm_vg_name TEXT,
            lvm_lv_uuid TEXT,
            lvm_lv_name TEXT,
            lvm_pv_offsets_json TEXT,
            lvm_pv_sources_json TEXT
        );",
    )
    .unwrap();

    let result = migrate_partitions(&conn).unwrap();
    assert_eq!(result.migrated_count, 0);
}
