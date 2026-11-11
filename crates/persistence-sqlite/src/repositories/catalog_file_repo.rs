use crate::connection::{DbError, DbResult};
use crate::repositories::file_repo::FileRepo;
use domain::FileEntry;
use rusqlite::Connection;

pub struct CatalogFileRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CatalogFileRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_batch_with_partition_index_in_transaction(
        &self,
        entries: &[FileEntry],
        partition_index: usize,
    ) -> DbResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        if crate::source_fjall::upsert_files_with_partition(self.conn, entries, partition_index)? {
            return Ok(());
        }
        Err(DbError::System("source detail storage is missing".into()))
    }

    pub fn update_root_in_transaction(&self, entry: &FileEntry) -> DbResult<()> {
        if crate::source_fjall::open_store(self.conn)?.is_some() {
            return FileRepo::new(self.conn).update_root_metadata(entry);
        }
        Err(DbError::System("source detail storage is missing".into()))
    }
}
