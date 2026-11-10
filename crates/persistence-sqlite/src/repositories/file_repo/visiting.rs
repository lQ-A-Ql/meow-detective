use super::FileRepo;
use crate::{DbError, DbResult};
use domain::FileEntry;

impl FileRepo<'_> {
    pub fn visit_all(
        &self,
        source: Option<&str>,
        mut visit: impl FnMut(FileEntry) -> DbResult<()>,
    ) -> DbResult<()> {
        if let Some(owner) = crate::source_fjall::source_id(self.conn)? {
            let store = crate::source_fjall::require_source_store(self.conn)?;
            if source.is_some_and(|source| source != owner) {
                return Err(DbError::System("source catalog owner mismatch".into()));
            }
            let catalog =
                persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
            let snapshot = catalog.read_snapshot();
            let mut after = None;
            loop {
                crate::read_operation::ensure_active()?;
                let page = catalog
                    .list_by_id(&snapshot, &owner, after.as_deref(), 512)
                    .map_err(|error| DbError::System(error.to_string()))?;
                for record in page.records {
                    after = Some(record.id.clone());
                    visit(crate::source_fjall::file_entry(record))?;
                }
                if page.next_cursor.is_none() {
                    return Ok(());
                }
            }
        }
        Err(DbError::System("source identity is missing".into()))
    }

    pub(super) fn native_count(&self, source: Option<&str>) -> DbResult<Option<u64>> {
        let Some(owner) = crate::source_fjall::source_id(self.conn)? else {
            return Ok(None);
        };
        let store = crate::source_fjall::require_source_store(self.conn)?;
        if source.is_some_and(|source| source != owner) {
            return Err(DbError::System("source catalog owner mismatch".into()));
        }
        let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        catalog
            .count(&catalog.read_snapshot(), source)
            .map(Some)
            .map_err(|error| DbError::System(error.to_string()))
    }
}
