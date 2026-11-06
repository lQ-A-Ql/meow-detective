use super::{
    mapping::{collect_entries, row_to_file_entry},
    FileRepo, FILE_ENTRY_COLUMNS,
};
use crate::DbResult;
use domain::FileEntry;
use rusqlite::params;
impl FileRepo<'_> {
    pub fn index_page(&self, source: &str, after: &str, limit: u32) -> DbResult<Vec<FileEntry>> {
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            if crate::source_fjall::source_id(self.conn)?.as_deref() != Some(source) {
                return Err(crate::DbError::System(
                    "source catalog owner mismatch".into(),
                ));
            }
            let catalog =
                persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
            return catalog
                .list_by_id(
                    &catalog.read_snapshot(),
                    source,
                    Some(after),
                    limit.min(1000) as usize,
                )
                .map(|page| {
                    page.records
                        .into_iter()
                        .map(crate::source_fjall::file_entry)
                        .collect()
                })
                .map_err(|error| crate::DbError::System(error.to_string()));
        }
        let mut statement=self.conn.prepare(&format!("SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id=?1 AND id>?2 ORDER BY id LIMIT ?3"))?;
        let rows =
            statement.query_map(params![source, after, limit.min(1000)], row_to_file_entry)?;
        collect_entries(rows)
    }
}
