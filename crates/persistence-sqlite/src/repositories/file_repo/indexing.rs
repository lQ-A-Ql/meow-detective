use super::FileRepo;
use crate::DbResult;
use domain::FileEntry;
impl FileRepo<'_> {
    pub fn index_page(&self, source: &str, after: &str, limit: u32) -> DbResult<Vec<FileEntry>> {
        if let Some(owner) = crate::source_fjall::source_id(self.conn)? {
            if owner != source {
                return Err(crate::DbError::System(
                    "source catalog owner mismatch".into(),
                ));
            }
            let store = crate::source_fjall::require_source_store(self.conn)?;
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
        Err(crate::DbError::System("source identity is missing".into()))
    }
}
