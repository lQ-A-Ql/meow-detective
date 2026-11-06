use crate::connection::DbResult;
use domain::FileEntry;
use rusqlite::Connection;

mod browse;
mod browse_paging;
mod browse_partition;
mod indexing;
mod lookup;
pub(crate) mod mapping;
mod registry_hives;
mod writes;

pub use browse_paging::{FileSortDirection, FileSortKey};
pub use mapping::file_encryption_status_from_row;

pub(super) const FILE_ENTRY_COLUMNS: &str = "id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted, read_only, archive, unix_mode";

pub struct FileRepo<'a> {
    pub(super) conn: &'a Connection,
}

impl<'a> FileRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub(super) fn fjall_entries(
        &self,
        source_id: Option<&str>,
    ) -> DbResult<Option<Vec<FileEntry>>> {
        let Some(configured_source_id) = crate::source_fjall::source_id(self.conn)? else {
            return Ok(None);
        };
        let source_id = source_id.unwrap_or(&configured_source_id);
        if source_id != configured_source_id {
            return Err(crate::DbError::System(
                "source catalog owner mismatch".into(),
            ));
        }
        let Some(store) = crate::source_fjall::open_store(self.conn)? else {
            return Ok(None);
        };
        let repository = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = repository.read_snapshot();
        let mut cursor = None;
        let mut entries = Vec::new();
        loop {
            let page = repository
                .list_all(
                    &snapshot,
                    source_id,
                    cursor.as_deref(),
                    persistence_fjall::ScanDirection::Forward,
                    512,
                )
                .map_err(|error| crate::connection::DbError::System(error.to_string()))?;
            entries.extend(
                page.records
                    .into_iter()
                    .map(crate::source_fjall::file_entry),
            );
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(Some(entries))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/file_repo.rs"]
mod tests;
