use super::{AnalysisServiceError, FileEntry};
use persistence_fjall::{CatalogRecordStore, ScanDirection};
use rusqlite::Connection;

pub(super) fn collect_file_entries_from_fjall(
    conn: &Connection,
) -> Result<Option<Vec<FileEntry>>, AnalysisServiceError> {
    let Some(store) = persistence_sqlite::source_fjall::open_store(conn)? else {
        return Ok(None);
    };
    let owner = persistence_sqlite::source_fjall::source_id(conn)?
        .ok_or_else(|| AnalysisServiceError::InvalidInput("source identity is missing".into()))?;
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut entries = Vec::new();
    let mut cursor = None;
    loop {
        persistence_sqlite::read_operation::ensure_active()?;
        let page = catalog
            .list_all(
                &snapshot,
                &owner,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        entries.extend(
            page.records
                .into_iter()
                .map(crate::file_service::record_to_domain),
        );
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(Some(entries));
        }
    }
}
