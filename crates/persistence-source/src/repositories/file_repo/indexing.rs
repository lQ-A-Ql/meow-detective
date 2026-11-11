use super::FileRepo;
use domain::FileEntry;
use persistence_sqlite::DbResult;
impl FileRepo<'_> {
    pub fn index_page(&self, source: &str, after: &str, limit: u32) -> DbResult<Vec<FileEntry>> {
        if let Some(owner) = crate::detail::source_id(self.conn)? {
            if owner != source {
                return Err(persistence_sqlite::DbError::System(
                    "source catalog owner mismatch".into(),
                ));
            }
            let store = crate::detail::require_source_store(self.conn)?;
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
                        .map(crate::detail::file_entry)
                        .collect()
                })
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()));
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }
}
