use super::FileRepo;
use domain::{DataSourceId, FileEntry, FileEntryId};
use persistence_sqlite::{DbError, DbResult};

impl FileRepo<'_> {
    pub(super) fn native_partition_path(
        &self,
        source: &DataSourceId,
        partition: usize,
        paths: &[String],
        case_insensitive: bool,
    ) -> DbResult<Option<Option<FileEntry>>> {
        let Some(owner) = crate::detail::source_id(self.conn)? else {
            return Ok(None);
        };
        if owner != source.0 {
            return Err(DbError::System("catalog source identity mismatch".into()));
        }
        let store = crate::detail::require_source_store(self.conn)?;
        let partition = u32::try_from(partition)
            .map_err(|_| DbError::System("partition index exceeds Fjall range".into()))?;
        let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = catalog.read_snapshot();
        for path in paths {
            let mut cursor = None;
            loop {
                let page = if case_insensitive {
                    catalog.list_all(
                        &snapshot,
                        &source.0,
                        cursor.as_deref(),
                        persistence_fjall::ScanDirection::Forward,
                        512,
                    )
                } else {
                    catalog.find_by_path(
                        &snapshot,
                        &source.0,
                        path,
                        cursor.as_deref(),
                        persistence_fjall::ScanDirection::Forward,
                        512,
                    )
                }
                .map_err(|error| DbError::System(error.to_string()))?;
                for record in page.records {
                    if !record.deleted
                        && record.partition_index == Some(partition)
                        && (record.path == *path
                            || (case_insensitive && record.path.eq_ignore_ascii_case(path)))
                    {
                        return Ok(Some(Some(crate::detail::file_entry(record))));
                    }
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
        }
        Ok(Some(None))
    }
    pub fn find_mount_children_for_partition(
        &self,
        parent: &FileEntryId,
        source: &DataSourceId,
        partition: usize,
    ) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.native_partition_entries(source, Some(parent), partition)? {
            entries.sort_by(|a, b| {
                b.is_directory()
                    .cmp(&a.is_directory())
                    .then_with(|| {
                        a.name
                            .to_ascii_lowercase()
                            .cmp(&b.name.to_ascii_lowercase())
                    })
                    .then_with(|| a.id.0.cmp(&b.id.0))
            });
            return Ok(entries);
        }
        Err(DbError::System("source identity is missing".into()))
    }

    pub fn find_root_for_partition(
        &self,
        source: &DataSourceId,
        partition: usize,
    ) -> DbResult<Option<FileEntry>> {
        if let Some(entries) = self.native_partition_entries(source, None, partition)? {
            return Ok(entries.into_iter().min_by(|a, b| a.id.0.cmp(&b.id.0)));
        }
        Err(DbError::System("source identity is missing".into()))
    }

    pub fn find_children_page_for_partition(
        &self,
        parent: &FileEntryId,
        source: &DataSourceId,
        partition: usize,
        offset: u64,
        limit: u32,
    ) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.native_partition_entries(source, Some(parent), partition)? {
            entries.sort_by(|a, b| {
                b.is_directory()
                    .cmp(&a.is_directory())
                    .then_with(|| a.name.cmp(&b.name))
                    .then_with(|| a.id.0.cmp(&b.id.0))
            });
            return Ok(entries
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect());
        }
        Err(DbError::System("source identity is missing".into()))
    }

    fn native_partition_entries(
        &self,
        source: &DataSourceId,
        parent: Option<&FileEntryId>,
        partition: usize,
    ) -> DbResult<Option<Vec<FileEntry>>> {
        let owner = crate::detail::source_id(self.conn)?
            .ok_or_else(|| DbError::System("source identity is missing".into()))?;
        if owner != source.0 {
            return Err(DbError::System("catalog source identity mismatch".into()));
        }
        let store = crate::detail::require_source_store(self.conn)?;
        let partition = u32::try_from(partition)
            .map_err(|_| DbError::System("partition index exceeds Fjall range".into()))?;
        let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = catalog.read_snapshot();
        let mut entries = Vec::new();
        let mut cursor = None;
        loop {
            let page = catalog
                .list_by_parent(
                    &snapshot,
                    &source.0,
                    parent.map(|id| id.0.as_str()),
                    cursor.as_deref(),
                    persistence_fjall::ScanDirection::Forward,
                    512,
                )
                .map_err(|error| DbError::System(error.to_string()))?;
            entries.extend(
                page.records
                    .into_iter()
                    .filter(|record| record.partition_index == Some(partition) && !record.deleted)
                    .map(crate::detail::file_entry),
            );
            if entries.len() > super::MAX_FILE_QUERY_ROWS {
                return Err(DbError::System(
                    "partition catalog query exceeds the bounded row limit".into(),
                ));
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(Some(entries));
            }
        }
    }
}
