use domain::{normalize_content_sha256, FileEntry, FileEntryId};
use persistence_sqlite::connection::DbResult;

use super::FileRepo;

impl FileRepo<'_> {
    pub fn update_hash_sha256(&self, id: &FileEntryId, hash: &str) -> DbResult<Option<FileEntry>> {
        let hash = normalize_content_sha256(Some(hash)).ok_or_else(|| {
            persistence_sqlite::connection::DbError::System(
                "file SHA-256 must be hexadecimal".into(),
            )
        })?;
        if let Some(store) = crate::detail::open_store_for_write(self.conn)? {
            let source_id = crate::detail::source_id(self.conn)?.ok_or_else(|| {
                persistence_sqlite::DbError::System("source identity is missing".into())
            })?;
            let catalog =
                persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
            let Some(mut record) = catalog
                .get(&catalog.read_snapshot(), &source_id, &id.0)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?
            else {
                return Ok(None);
            };
            record.hash_sha256 = Some(hash);
            catalog
                .upsert(&record, persistence_fjall::PersistMode::SyncAll)
                .map_err(|error| {
                    persistence_sqlite::connection::DbError::System(error.to_string())
                })?;
            return Ok(Some(crate::detail::file_entry(record)));
        }
        Err(persistence_sqlite::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn remove_partition_placeholder_root(
        &self,
        data_source_id: &str,
        path_pattern: &str,
    ) -> DbResult<usize> {
        if let Some(store) = crate::detail::open_store_for_write(self.conn)? {
            let source = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
            let snapshot = source.read_snapshot();
            let mut deleted = 0;
            let mut cursor = None;
            loop {
                let page = source
                    .list_by_parent(
                        &snapshot,
                        data_source_id,
                        None,
                        cursor.as_deref(),
                        persistence_fjall::ScanDirection::Forward,
                        512,
                    )
                    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                for record in page.records {
                    if wildcard_match(path_pattern, &record.path)
                        && source
                            .delete(
                                data_source_id,
                                &record.id,
                                persistence_fjall::PersistMode::Buffer,
                            )
                            .map_err(|error| {
                                persistence_sqlite::DbError::System(error.to_string())
                            })?
                    {
                        deleted += 1;
                    }
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            store
                .database()
                .persist(persistence_fjall::PersistMode::SyncAll)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            return Ok(deleted);
        }
        Err(persistence_sqlite::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn update_root_metadata(&self, entry: &FileEntry) -> DbResult<()> {
        if let Some(store) = crate::detail::open_store_for_write(self.conn)? {
            let catalog =
                persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
            let previous = catalog
                .get(
                    &catalog.read_snapshot(),
                    &entry.data_source_id.0,
                    &entry.id.0,
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?
                .ok_or_else(|| {
                    persistence_sqlite::DbError::System("catalog root is missing".into())
                })?;
            let mut record = crate::detail::file_record(entry);
            record.partition_index = previous.partition_index;
            record.encrypted = previous.encrypted;
            catalog
                .upsert(&record, persistence_fjall::PersistMode::SyncAll)
                .map_err(|error| {
                    persistence_sqlite::connection::DbError::System(error.to_string())
                })?;
            return Ok(());
        }
        Err(persistence_sqlite::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn insert_batch(&self, entries: &[FileEntry]) -> DbResult<()> {
        let transaction = self.conn.unchecked_transaction()?;
        FileRepo::new(&transaction).insert_batch_unchecked(entries)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_batch_unchecked(&self, entries: &[FileEntry]) -> DbResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        if crate::detail::upsert_files(self.conn, entries)? {
            return Ok(());
        }
        Err(persistence_sqlite::DbError::System(
            "source detail storage is missing".into(),
        ))
    }
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let Some(prefix) = pattern.strip_suffix('*') else {
        return pattern == value;
    };
    value.starts_with(prefix)
}
