use super::{
    mapping::{collect_entries, row_to_file_entry},
    FileRepo, FILE_ENTRY_COLUMNS,
};
use crate::{DbError, DbResult};
use domain::{DataSourceId, FileEntry, FileEntryId};
use rusqlite::{params, OptionalExtension};

impl FileRepo<'_> {
    pub(super) fn native_partition_path(
        &self,
        source: &DataSourceId,
        partition: usize,
        paths: &[String],
        case_insensitive: bool,
    ) -> DbResult<Option<Option<FileEntry>>> {
        let Some(store) = crate::source_fjall::open_store(self.conn)? else {
            return Ok(None);
        };
        if crate::source_fjall::source_id(self.conn)?.as_deref() != Some(&source.0) {
            return Err(DbError::System("catalog source identity mismatch".into()));
        }
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
                        return Ok(Some(Some(crate::source_fjall::file_entry(record))));
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
        let mut statement = self.conn.prepare_cached(&format!("SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id=?1 AND data_source_id=?2 AND partition_index=?3 AND deleted=0 ORDER BY entry_type ASC,name COLLATE NOCASE ASC,id ASC"))?;
        let rows = statement.query_map(
            params![parent.0, source.0, partition as u64],
            row_to_file_entry,
        )?;
        collect_entries(rows)
    }

    pub fn find_root_for_partition(
        &self,
        source: &DataSourceId,
        partition: usize,
    ) -> DbResult<Option<FileEntry>> {
        if let Some(entries) = self.native_partition_entries(source, None, partition)? {
            return Ok(entries.into_iter().min_by(|a, b| a.id.0.cmp(&b.id.0)));
        }
        self.conn.query_row(&format!("SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id IS NULL AND data_source_id=?1 AND partition_index=?2 AND deleted=0 ORDER BY id ASC LIMIT 1"),params![source.0,partition as u64],row_to_file_entry).optional().map_err(Into::into)
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
        let mut statement = self.conn.prepare(&format!("SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id=?1 AND data_source_id=?2 AND partition_index=?3 AND deleted=0 ORDER BY entry_type ASC,name ASC LIMIT ?4 OFFSET ?5"))?;
        let rows = statement.query_map(
            params![parent.0, source.0, partition as u64, limit, offset],
            row_to_file_entry,
        )?;
        collect_entries(rows)
    }

    fn native_partition_entries(
        &self,
        source: &DataSourceId,
        parent: Option<&FileEntryId>,
        partition: usize,
    ) -> DbResult<Option<Vec<FileEntry>>> {
        let Some(store) = crate::source_fjall::open_store(self.conn)? else {
            return Ok(None);
        };
        if crate::source_fjall::source_id(self.conn)?.as_deref() != Some(&source.0) {
            return Err(DbError::System("catalog source identity mismatch".into()));
        }
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
                    .map(crate::source_fjall::file_entry),
            );
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(Some(entries));
            }
        }
    }
}
