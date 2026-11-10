use super::{AnalysisServiceError, FileEntry};
use persistence_fjall::CatalogRecordStore;
use rusqlite::Connection;

pub(super) fn collect_file_entries_from_fjall(
    conn: &Connection,
) -> Result<Vec<FileEntry>, AnalysisServiceError> {
    let store = persistence_sqlite::source_fjall::open_store(conn)?.ok_or_else(|| {
        AnalysisServiceError::InvalidInput("source detail storage is missing".into())
    })?;
    let owner = persistence_sqlite::source_fjall::source_id(conn)?
        .ok_or_else(|| AnalysisServiceError::InvalidInput("source identity is missing".into()))?;
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut entries = Vec::new();
    let mut after: Option<(String, String)> = None;
    loop {
        persistence_sqlite::read_operation::ensure_active()?;
        let page = catalog
            .list_files_by_path(
                &snapshot,
                &owner,
                after
                    .as_ref()
                    .map(|(path, id)| (path.as_str(), id.as_str())),
                512,
            )
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        let next_after = page
            .records
            .last()
            .map(|record| (record.path.clone(), record.id.clone()));
        entries.extend(
            page.records
                .into_iter()
                .map(crate::file_service::record_to_domain),
        );
        if page.next_cursor.is_none() {
            return Ok(entries);
        }
        after = Some(next_after.expect("a non-terminal catalog page contains a record"));
    }
}
