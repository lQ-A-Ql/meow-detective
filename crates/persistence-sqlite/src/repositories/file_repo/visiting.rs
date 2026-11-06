use super::{mapping::row_to_file_entry, FileRepo, FILE_ENTRY_COLUMNS};
use crate::{DbError, DbResult};
use domain::FileEntry;

impl FileRepo<'_> {
    pub fn visit_all(
        &self,
        source: Option<&str>,
        mut visit: impl FnMut(FileEntry) -> DbResult<()>,
    ) -> DbResult<()> {
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            let owner = crate::source_fjall::source_id(self.conn)?
                .ok_or_else(|| DbError::System("source identity is missing".into()))?;
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
        let mut statement = self.conn.prepare(&format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE ?1 IS NULL OR data_source_id=?1 ORDER BY id"
        ))?;
        for row in statement.query_map([source], row_to_file_entry)? {
            crate::read_operation::ensure_active()?;
            visit(row?)?;
        }
        Ok(())
    }

    pub(super) fn native_count(&self, source: Option<&str>) -> DbResult<Option<u64>> {
        let Some(store) = crate::source_fjall::open_store(self.conn)? else {
            return Ok(None);
        };
        let owner = crate::source_fjall::source_id(self.conn)?
            .ok_or_else(|| DbError::System("source identity is missing".into()))?;
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
