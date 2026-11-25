//! Bounded registry companion lookup through the source path index.
use super::FileRepo;
use domain::{DataSourceId, FileEntry};
use persistence_sqlite::DbResult;

impl FileRepo<'_> {
    pub fn registry_log_companions(
        &self,
        source: &DataSourceId,
        hive_path: &str,
    ) -> DbResult<Vec<FileEntry>> {
        let owner = crate::detail::source_id(self.conn)?.ok_or_else(|| {
            persistence_sqlite::DbError::System("source identity is missing".into())
        })?;
        if owner != source.0 {
            return Err(persistence_sqlite::DbError::System(
                "source catalog owner mismatch".into(),
            ));
        }
        let path = hive_path.replace('\\', "/");
        let mut entries = Vec::new();
        for candidate in [format!("{path}.LOG1"), format!("{path}.LOG2")] {
            entries.extend(
                self.exact_path_entries(&source.0, &candidate)?
                    .into_iter()
                    .filter(|entry| !entry.deleted && !entry.is_directory())
                    .take(2),
            );
        }
        Ok(entries)
    }

    fn exact_path_entries(&self, source: &str, path: &str) -> DbResult<Vec<FileEntry>> {
        let store = crate::detail::require_source_store(self.conn)?;
        let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = catalog.read_snapshot();
        let page = catalog
            .list_files_by_path(&snapshot, source, Some((path, "")), 64)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        Ok(page
            .records
            .into_iter()
            .filter(|record| record.path.eq_ignore_ascii_case(path))
            .map(crate::detail::file_entry)
            .collect())
    }
}
