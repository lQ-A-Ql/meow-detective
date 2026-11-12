#![allow(dead_code)]

use app_services::staging::{PartitionEntry, PartitionStatus, StagingManifest};
use persistence_source::fjall::{FileEntryRecord, ScanDirection};
use rusqlite::{params, Connection};

pub struct NativeSourceFixture {
    pub conn: Connection,
    pub storage: app_services::source_db::FjallReadWriteSourceStorageHandle,
    pub source_id: String,
}

impl NativeSourceFixture {
    pub fn new(case_root: &std::path::Path, source_id: &str) -> Self {
        let id = domain::DataSourceId(source_id.to_string());
        let conn = app_services::source_db::open_source_db(case_root, &id).unwrap();
        conn.execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES (?1, 'case-1', ?1, 'e01', 'source.E01',
                     '2026-01-01T00:00:00Z')",
            [source_id],
        )
        .unwrap();
        let storage =
            app_services::source_db::open_fjall_source_read_write(case_root, source_id).unwrap();
        Self {
            conn,
            storage,
            source_id: source_id.to_string(),
        }
    }

    pub fn catalog_records(&self) -> Vec<FileEntryRecord> {
        catalog_records(&self.conn)
    }
}

pub fn done_partition(index: usize, name: &str, file_count: u64) -> PartitionEntry {
    PartitionEntry {
        index,
        name: name.to_string(),
        fs_kind: "Ntfs".to_string(),
        staging_db: format!("enum_partition_{index}.db"),
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

pub fn catalog_records(conn: &Connection) -> Vec<FileEntryRecord> {
    let source_id = persistence_source::detail::source_id(conn)
        .unwrap()
        .expect("source identity");
    let store = persistence_source::detail::open_store(conn)
        .unwrap()
        .expect("source store");
    let catalog =
        persistence_source::fjall::CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = catalog
            .list_all(
                &snapshot,
                &source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .unwrap();
        records.extend(page.records);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));
    records
}

pub fn first_level_roots(main_conn: &Connection) -> Vec<String> {
    let mut roots = catalog_records(main_conn)
        .into_iter()
        .filter(|record| record.parent_id.is_none())
        .map(|record| record.name)
        .collect::<Vec<_>>();
    roots.sort();
    roots
}

pub fn attached_db_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn.prepare("PRAGMA database_list").unwrap();
    stmt.query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
