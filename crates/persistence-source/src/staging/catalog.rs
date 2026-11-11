use crate::detail::{file_record, open_store_for_write};
use persistence_fjall::{
    CatalogEntryType, CatalogRecordStore, FileEntryRecord, PersistMode, ScanDirection,
};
use persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo;
use persistence_sqlite::{DbError, DbResult};
use rusqlite::{Connection, Row};

const PAGE_SIZE: usize = 512;

pub fn merge_staging_catalog(
    main: &Connection,
    staging: &Connection,
    source_id: &str,
    partition: usize,
    partition_name: &str,
) -> DbResult<Option<u64>> {
    let Some(store) = open_store_for_write(main)? else {
        return Ok(None);
    };
    if crate::detail::source_id(main)?.as_deref() != Some(source_id) {
        return Err(DbError::System(
            "catalog staging source identity mismatch".into(),
        ));
    }
    let partition = u32::try_from(partition)
        .map_err(|_| DbError::System("partition index exceeds Fjall range".into()))?;
    validate_staging(staging, source_id, partition)?;
    let repository = CatalogRecordStore::from_source_store((*store).clone());
    let root = partition_root(&repository, source_id, partition, partition_name)?;
    let snapshot = repository.read_snapshot();
    let transaction = staging.unchecked_transaction()?;
    let mut statement = transaction.prepare(include_str!("catalog_staging.sql"))?;
    let mut rows = statement.query([&root.id])?;
    let mut batch = Vec::with_capacity(PAGE_SIZE);
    batch.push(root.clone());
    let mut merged = 0;
    while let Some(row) = rows.next()? {
        persistence_sqlite::read_operation::ensure_active()?;
        let record = record(row, source_id, partition)?;
        if let Some(previous) = repository
            .get(&snapshot, source_id, &record.id)
            .map_err(storage_error)?
        {
            if previous.partition_index != Some(partition) {
                return Err(DbError::System(
                    "catalog entry id conflicts with another partition".into(),
                ));
            }
        }
        batch.push(record);
        merged += 1;
        if batch.len() == PAGE_SIZE {
            flush(main, &repository, &batch)?;
            batch.clear();
        }
    }
    flush(main, &repository, &batch)?;
    store
        .database()
        .persist(PersistMode::SyncAll)
        .map_err(storage_error)?;
    Ok(Some(merged))
}

fn flush(
    _conn: &Connection,
    catalog: &CatalogRecordStore,
    records: &[FileEntryRecord],
) -> DbResult<()> {
    catalog
        .upsert_batch(records, PersistMode::Buffer)
        .map_err(storage_error)
}

fn validate_staging(staging: &Connection, source: &str, partition: u32) -> DbResult<()> {
    if NtfsStagingRepo::is_numeric(staging)? {
        if NtfsStagingRepo::meta(staging, "source_id")?.as_deref() != Some(source)
            || NtfsStagingRepo::meta(staging, "partition")?.as_deref()
                != Some(partition.to_string().as_str())
        {
            return Err(DbError::System("numeric staging identity mismatch".into()));
        }
        NtfsStagingRepo::validate_paths(staging)?;
    }
    let foreign: bool = staging.query_row(
        "SELECT EXISTS(SELECT 1 FROM file_entries WHERE data_source_id <> ?1
         OR (partition_index IS NOT NULL AND partition_index <> ?2))",
        rusqlite::params![source, partition],
        |row| row.get(0),
    )?;
    if foreign {
        return Err(DbError::System(
            "staging rows contain a different source or partition".into(),
        ));
    }
    Ok(())
}

fn partition_root(
    repository: &CatalogRecordStore,
    source: &str,
    partition: u32,
    name: &str,
) -> DbResult<FileEntryRecord> {
    let snapshot = repository.read_snapshot();
    let mut cursor = None;
    let mut root = None;
    loop {
        let page = repository
            .list_by_parent(
                &snapshot,
                source,
                None,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(storage_error)?;
        for record in page.records {
            if record.partition_index == Some(partition) {
                if root.is_some() {
                    return Err(DbError::System(
                        "partition has multiple catalog roots".into(),
                    ));
                }
                root = Some(record);
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let mut root = root.unwrap_or_else(|| FileEntryRecord {
        id: format!("partition-root:{source}:{partition}"),
        parent_id: None,
        data_source_id: source.into(),
        path: String::new(),
        name: name.into(),
        entry_type: CatalogEntryType::Directory,
        size: None,
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        encrypted: Some(false),
        created: None,
        modified: None,
        accessed: None,
        changed: None,
        hash_sha256: None,
        partition_index: Some(partition),
        unix_mode: None,
    });
    root.path.clear();
    root.name = name.into();
    Ok(root)
}

fn record(row: &Row<'_>, source: &str, partition: u32) -> DbResult<FileEntryRecord> {
    let entry = persistence_sqlite::repositories::file_repo::mapping::row_to_file_entry(row)?;
    if entry.data_source_id.0 != source {
        return Err(DbError::System(
            "staging row source identity mismatch".into(),
        ));
    }
    let mut record = file_record(&entry);
    record.encrypted = row.get::<_, Option<i64>>(16)?.map(|value| value != 0);
    record.partition_index = Some(partition);
    Ok(record)
}

fn storage_error(error: persistence_fjall::FjallStorageError) -> DbError {
    DbError::System(error.to_string())
}
