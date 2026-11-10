use std::collections::HashMap;

use crate::connection::DbResult;
use domain::{DataSourceId, FileEntry, FileEntryId};

use super::FileRepo;

impl FileRepo<'_> {
    pub fn find_children(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries_by_parent(None, Some(&parent_id.0))? {
            sort_entries(&mut entries);
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
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
        Err(crate::DbError::System("source identity is missing".into()))
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
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn count_children(&self, parent_id: &FileEntryId) -> DbResult<u64> {
        if let Some(entries) = self.fjall_entries_by_parent(None, Some(&parent_id.0))? {
            return Ok(entries.len() as u64);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_root_entries(&self) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries_by_parent(None, None)? {
            sort_entries(&mut entries);
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
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
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_root_entries_visible(&self, show_hidden: bool) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_root_entries();
        }
        if let Some(mut entries) = self.find_root_entries().map(Some)? {
            entries.retain(|entry| !entry.hidden && !entry.system);
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn count_root_entries(&self) -> DbResult<u64> {
        if let Some(entries) = self.fjall_entries_by_parent(None, None)? {
            return Ok(entries.len() as u64);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_child_directories(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.find_children(parent_id).map(Some)? {
            entries.retain(FileEntry::is_directory);
            entries.sort_by(|left, right| left.name.cmp(&right.name));
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
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
        Err(crate::DbError::System("source identity is missing".into()))
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
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn count_child_directories_visible(
        &self,
        parent_id: &FileEntryId,
        show_hidden: bool,
    ) -> DbResult<u64> {
        if let Some(entries) = self
            .find_child_directories_visible(parent_id, show_hidden)
            .map(Some)?
        {
            return Ok(entries.len() as u64);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_child_directories_page_visible(
        &self,
        parent_id: &FileEntryId,
        offset: u64,
        limit: u32,
        show_hidden: bool,
    ) -> DbResult<Vec<FileEntry>> {
        if let Some(entries) = self
            .find_child_directories_visible(parent_id, show_hidden)
            .map(Some)?
        {
            return Ok(entries
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect());
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn child_directories_need_natural_sort(
        &self,
        parent_id: &FileEntryId,
        show_hidden: bool,
    ) -> DbResult<bool> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            return Ok(self
                .find_child_directories_visible(parent_id, show_hidden)?
                .iter()
                .any(|entry| entry.name.bytes().any(|byte| byte.is_ascii_digit())));
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn count_child_directories(&self, parent_id: &FileEntryId) -> DbResult<u64> {
        if let Some(entries) = self.find_child_directories(parent_id).map(Some)? {
            return Ok(entries.len() as u64);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_roots(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries(Some(&data_source_id.0))? {
            entries.retain(|entry| entry.parent_id.is_none());
            sort_entries(&mut entries);
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_by_data_source(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries(Some(&data_source_id.0))? {
            sort_entries(&mut entries);
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_root_directories(&self) -> DbResult<Vec<FileEntry>> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
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
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_root_directories_visible(&self, show_hidden: bool) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_root_directories();
        }
        if let Some(mut entries) = self.find_root_directories().map(Some)? {
            entries.retain(|entry| !entry.hidden && !entry.system);
            return Ok(entries);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn has_child_directories(&self, parent_id: &FileEntryId) -> DbResult<bool> {
        if let Some(entries) = self.find_child_directories(parent_id).map(Some)? {
            return Ok(!entries.is_empty());
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn count_child_directories_batch(
        &self,
        parent_ids: &[&FileEntryId],
    ) -> DbResult<HashMap<String, i64>> {
        self.count_child_directories_batch_filtered(parent_ids, false)
    }

    pub fn count_child_directories_batch_visible(
        &self,
        parent_ids: &[&FileEntryId],
        show_hidden: bool,
    ) -> DbResult<HashMap<String, i64>> {
        if show_hidden {
            return self.count_child_directories_batch(parent_ids);
        }
        self.count_child_directories_batch_filtered(parent_ids, true)
    }

    fn count_child_directories_batch_filtered(
        &self,
        parent_ids: &[&FileEntryId],
        visible_only: bool,
    ) -> DbResult<HashMap<String, i64>> {
        if parent_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let owner = crate::source_fjall::source_id(self.conn)?
            .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
        let store = crate::source_fjall::require_source_store(self.conn)?;
        let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = catalog.read_snapshot();
        let mut result = HashMap::new();
        for parent_id in parent_ids {
            let mut cursor = None;
            let mut count = 0u64;
            loop {
                crate::read_operation::ensure_active()?;
                let page = catalog
                    .list_directories_by_parent(
                        &snapshot,
                        &owner,
                        Some(&parent_id.0),
                        cursor.as_deref(),
                        persistence_fjall::ScanDirection::Forward,
                        512,
                    )
                    .map_err(|error| crate::DbError::System(error.to_string()))?;
                count = count.saturating_add(
                    page.records
                        .iter()
                        .filter(|record| {
                            !record.deleted && (!visible_only || (!record.hidden && !record.system))
                        })
                        .count() as u64,
                );
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            if count > 0 {
                let count = i64::try_from(count).map_err(|_| {
                    crate::DbError::System("child directory count exceeds SQLite range".into())
                })?;
                result.insert(parent_id.0.clone(), count);
            }
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
