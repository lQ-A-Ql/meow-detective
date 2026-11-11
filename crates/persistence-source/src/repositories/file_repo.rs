use domain::FileEntry;
use persistence_sqlite::connection::DbResult;
use rusqlite::Connection;

mod browse;
mod browse_paging;
mod browse_partition;
mod indexing;
mod lookup;
pub mod mapping;
mod registry_hives;
mod visiting;
mod writes;

pub(crate) const MAX_FILE_QUERY_ROWS: usize = 100_000;

pub(super) fn push_bounded<T>(rows: &mut Vec<T>, row: T) -> DbResult<()> {
    if rows.len() >= MAX_FILE_QUERY_ROWS {
        return Err(persistence_sqlite::DbError::System(
            "file catalog query exceeds the bounded row limit".into(),
        ));
    }
    rows.push(row);
    Ok(())
}

pub use browse_paging::{FileSortDirection, FileSortKey};
pub use mapping::file_encryption_status_from_row;

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
        let Some(configured_source_id) = crate::detail::source_id(self.conn)? else {
            return Ok(None);
        };
        let source_id = source_id.unwrap_or(&configured_source_id);
        if source_id != configured_source_id {
            return Err(persistence_sqlite::DbError::System(
                "source catalog owner mismatch".into(),
            ));
        }
        let Some(store) = crate::detail::open_store(self.conn)? else {
            if crate::detail::source_id(self.conn)?.is_some() {
                return Err(persistence_sqlite::DbError::System(
                    "source detail storage is missing".into(),
                ));
            }
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
                .map_err(|error| {
                    persistence_sqlite::connection::DbError::System(error.to_string())
                })?;
            entries.extend(page.records.into_iter().map(crate::detail::file_entry));
            if entries.len() > MAX_FILE_QUERY_ROWS {
                return Err(persistence_sqlite::DbError::System(
                    "file catalog query exceeds the bounded row limit".into(),
                ));
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(Some(entries))
    }

    pub(super) fn fjall_entries_by_parent(
        &self,
        source_id: Option<&str>,
        parent_id: Option<&str>,
    ) -> DbResult<Option<Vec<FileEntry>>> {
        let Some(configured_source_id) = crate::detail::source_id(self.conn)? else {
            return Ok(None);
        };
        let source_id = source_id.unwrap_or(&configured_source_id);
        if source_id != configured_source_id {
            return Err(persistence_sqlite::DbError::System(
                "source catalog owner mismatch".into(),
            ));
        }
        let store = crate::detail::require_source_store(self.conn)?;
        let repository = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = repository.read_snapshot();
        let mut cursor = None;
        let mut entries = Vec::new();
        loop {
            persistence_sqlite::read_operation::ensure_active()?;
            let page = repository
                .list_by_parent(
                    &snapshot,
                    source_id,
                    parent_id,
                    cursor.as_deref(),
                    persistence_fjall::ScanDirection::Forward,
                    512,
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            entries.extend(page.records.into_iter().map(crate::detail::file_entry));
            if entries.len() > MAX_FILE_QUERY_ROWS {
                return Err(persistence_sqlite::DbError::System(
                    "file catalog query exceeds the bounded row limit".into(),
                ));
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(Some(entries));
            }
        }
    }

    pub(super) fn fjall_child_page(
        &self,
        source_id: Option<&str>,
        parent_id: Option<&str>,
        offset: u64,
        limit: u32,
        directories_only: bool,
        visible_only: bool,
    ) -> DbResult<Option<Vec<FileEntry>>> {
        let Some(configured_source_id) = crate::detail::source_id(self.conn)? else {
            return Ok(None);
        };
        let source_id = source_id.unwrap_or(&configured_source_id);
        if source_id != configured_source_id {
            return Err(persistence_sqlite::DbError::System(
                "source catalog owner mismatch".into(),
            ));
        }
        let store = crate::detail::require_source_store(self.conn)?;
        let repository = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = repository.read_snapshot();
        let mut cursor = None;
        let mut skipped = 0u64;
        let mut entries = Vec::with_capacity(limit as usize);
        loop {
            persistence_sqlite::read_operation::ensure_active()?;
            let page = if directories_only {
                repository.list_directories_by_parent(
                    &snapshot,
                    source_id,
                    parent_id,
                    cursor.as_deref(),
                    persistence_fjall::ScanDirection::Forward,
                    512,
                )
            } else {
                repository.list_by_parent(
                    &snapshot,
                    source_id,
                    parent_id,
                    cursor.as_deref(),
                    persistence_fjall::ScanDirection::Forward,
                    512,
                )
            }
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            for record in page.records {
                if record.deleted || (visible_only && (record.hidden || record.system)) {
                    continue;
                }
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                entries.push(crate::detail::file_entry(record));
                if entries.len() == limit as usize {
                    return Ok(Some(entries));
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(Some(entries));
            }
        }
    }

    pub(super) fn fjall_orphan_directories(&self) -> DbResult<Option<Vec<FileEntry>>> {
        let Some(source_id) = crate::detail::source_id(self.conn)? else {
            return Ok(None);
        };
        let store = crate::detail::require_source_store(self.conn)?;
        let repository = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = repository.read_snapshot();
        let mut cursor = None;
        let mut entries = Vec::new();
        loop {
            persistence_sqlite::read_operation::ensure_active()?;
            let page = repository
                .list_orphan_directories(
                    &snapshot,
                    &source_id,
                    cursor.as_deref(),
                    persistence_fjall::ScanDirection::Forward,
                    512,
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            entries.extend(page.records.into_iter().map(crate::detail::file_entry));
            if entries.len() > MAX_FILE_QUERY_ROWS {
                return Err(persistence_sqlite::DbError::System(
                    "file catalog query exceeds the bounded row limit".into(),
                ));
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(Some(entries));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/file_repo.rs"]
mod tests;
