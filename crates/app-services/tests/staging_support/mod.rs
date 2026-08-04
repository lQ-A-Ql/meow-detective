#![allow(dead_code)]

use app_services::staging::{PartitionEntry, PartitionStatus, StagingManifest};
use rusqlite::{params, Connection};

pub fn create_main_file_entries_table(conn: &Connection) {
    persistence_sqlite::runner::run_source_all(conn).unwrap();
}

pub fn create_main_analysis_tables(conn: &Connection) {
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

pub fn done_partition(index: usize, name: &str, file_count: u64) -> PartitionEntry {
    PartitionEntry {
        index,
        name: name.to_string(),
        fs_kind: "Ntfs".to_string(),
        staging_db: format!("partition_{index}.db"),
        status: PartitionStatus::Done,
        file_count,
        dir_count: 0,
        total_size: 0,
        last_path: None,
        completed_at: None,
        error: None,
    }
}

pub fn single_done_manifest(ds_id: &str, name: &str) -> StagingManifest {
    let mut manifest = StagingManifest::create(ds_id, "/test.E01", "E01");
    manifest.partitions.push(done_partition(0, name, 0));
    manifest
}

pub fn insert_staging_row(
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

pub fn seed_placeholder(
    main_conn: &Connection,
    ds_id: &str,
    partition_index: usize,
    name: &str,
) -> String {
    app_services::file_service::insert_partition_placeholder_root(
        main_conn,
        &domain::DataSourceId(ds_id.to_string()),
        partition_index,
        name,
        "queued",
    )
    .unwrap()
    .0
}

pub fn first_level_roots(main_conn: &Connection) -> Vec<String> {
    let mut stmt = main_conn
        .prepare("SELECT name FROM file_entries WHERE parent_id IS NULL ORDER BY name")
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

pub fn attached_db_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn.prepare("PRAGMA database_list").unwrap();
    stmt.query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
