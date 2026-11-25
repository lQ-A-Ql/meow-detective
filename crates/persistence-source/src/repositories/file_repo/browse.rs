use std::collections::HashMap;

use domain::{DataSourceId, FileEntry, FileEntryId};
use persistence_sqlite::connection::DbResult;

use super::FileRepo;

impl FileRepo<'_> {
    pub fn find_children(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries_by_parent(None, Some(&parent_id.0))? {
            sort_entries(&mut entries);
            return Ok(entries);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_children_page(
        &self,
        parent_id: &FileEntryId,
        offset: u64,
        limit: u32,
    ) -> DbResult<Vec<FileEntry>> {
        if let Some(entries) = self.find_children(parent_id).map(Some)? {
            return Ok(entries
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect());
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_children_visible(
        &self,
        parent_id: &FileEntryId,
        show_hidden: bool,
    ) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_children(parent_id);
        }
        if let Some(mut entries) = self.find_children(parent_id).map(Some)? {
            entries.retain(|entry| !entry.hidden && !entry.system);
            return Ok(entries);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn count_children(&self, parent_id: &FileEntryId) -> DbResult<u64> {
        if let Some(entries) = self.fjall_entries_by_parent(None, Some(&parent_id.0))? {
            return Ok(entries.len() as u64);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_root_entries(&self) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries_by_parent(None, None)? {
            sort_entries(&mut entries);
            return Ok(entries);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_root_entries_page(&self, offset: u64, limit: u32) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries_by_parent(None, None)? {
            sort_entries(&mut entries);
            return Ok(entries
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect());
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_root_entries_visible(&self, show_hidden: bool) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_root_entries();
        }
        if let Some(mut entries) = self.find_root_entries().map(Some)? {
            entries.retain(|entry| !entry.hidden && !entry.system);
            return Ok(entries);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn count_root_entries(&self) -> DbResult<u64> {
        if let Some(entries) = self.fjall_entries_by_parent(None, None)? {
            return Ok(entries.len() as u64);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_child_directories(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.find_children(parent_id).map(Some)? {
            entries.retain(FileEntry::is_directory);
            entries.sort_by(|left, right| left.name.cmp(&right.name));
            return Ok(entries);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_child_directories_page(
        &self,
        parent_id: &FileEntryId,
        offset: u64,
        limit: u32,
    ) -> DbResult<Vec<FileEntry>> {
        if let Some(entries) = self.find_child_directories(parent_id).map(Some)? {
            return Ok(entries
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect());
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_child_directories_visible(
        &self,
        parent_id: &FileEntryId,
        show_hidden: bool,
    ) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_child_directories(parent_id);
        }
        if let Some(mut entries) = self.find_child_directories(parent_id).map(Some)? {
            entries.retain(|entry| !entry.hidden && !entry.system);
            return Ok(entries);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn count_child_directories_visible(
        &self,
        parent_id: &FileEntryId,
        show_hidden: bool,
    ) -> DbResult<u64> {
        Ok(self
            .indexed_page(Some(parent_id), 0, 0, show_hidden, None)?
            .total)
    }

    pub fn find_child_directories_page_visible(
        &self,
        parent_id: &FileEntryId,
        offset: u64,
        limit: u32,
        show_hidden: bool,
    ) -> DbResult<Vec<FileEntry>> {
        Ok(self
            .indexed_page(Some(parent_id), offset, limit, show_hidden, None)?
            .records
            .into_iter()
            .map(crate::detail::file_entry)
            .collect())
    }

    pub fn count_child_directories(&self, parent_id: &FileEntryId) -> DbResult<u64> {
        self.count_child_directories_visible(parent_id, true)
    }

    pub fn find_roots(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries(Some(&data_source_id.0))? {
            entries.retain(|entry| entry.parent_id.is_none());
            sort_entries(&mut entries);
            return Ok(entries);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_by_data_source(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries(Some(&data_source_id.0))? {
            sort_entries(&mut entries);
            return Ok(entries);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_root_directories(&self) -> DbResult<Vec<FileEntry>> {
        if crate::detail::source_id(self.conn)?.is_some() {
            let mut entries = self
                .fjall_entries_by_parent(None, None)?
                .unwrap_or_default()
                .into_iter()
                .filter(FileEntry::is_directory)
                .collect::<Vec<_>>();
            entries.extend(self.fjall_orphan_directories()?.unwrap_or_default());
            entries.sort_by(|left, right| left.id.0.cmp(&right.id.0));
            entries.dedup_by(|left, right| left.id == right.id);
            entries.sort_by(|left, right| left.name.cmp(&right.name));
            return Ok(entries);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn find_root_directories_visible(&self, show_hidden: bool) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_root_directories();
        }
        if let Some(mut entries) = self.find_root_directories().map(Some)? {
            entries.retain(|entry| !entry.hidden && !entry.system);
            return Ok(entries);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    pub fn has_child_directories(&self, parent_id: &FileEntryId) -> DbResult<bool> {
        Ok(self.count_child_directories_visible(parent_id, true)? > 0)
    }

    pub fn has_child_directories_batch_visible(
        &self,
        parent_ids: &[&FileEntryId],
        show_hidden: bool,
    ) -> DbResult<HashMap<String, bool>> {
        if parent_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let owner = crate::detail::source_id(self.conn)?.ok_or_else(|| {
            persistence_sqlite::DbError::System("source identity is missing".into())
        })?;
        let store = crate::detail::require_source_store(self.conn)?;
        let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = catalog.read_snapshot();
        let mut result = HashMap::new();
        for parent_id in parent_ids {
            persistence_sqlite::read_operation::ensure_active()?;
            let count = catalog
                .child_directory_count(&snapshot, &owner, Some(&parent_id.0), show_hidden)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            result.insert(parent_id.0.clone(), count > 0);
        }
        Ok(result)
    }
}

fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|left, right| {
        let left_directory = left.is_directory();
        let right_directory = right.is_directory();
        right_directory
            .cmp(&left_directory)
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
}
