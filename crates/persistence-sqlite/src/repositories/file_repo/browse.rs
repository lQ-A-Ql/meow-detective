use std::collections::HashMap;

use crate::{connection::DbResult, sql_builder::placeholders};
use domain::{DataSourceId, FileEntry, FileEntryId};
use rusqlite::params;

use super::{mapping::collect_entries, mapping::row_to_file_entry, FileRepo, FILE_ENTRY_COLUMNS};

impl FileRepo<'_> {
    pub fn find_children(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries(None)? {
            entries.retain(|entry| entry.parent_id.as_ref() == Some(parent_id));
            sort_entries(&mut entries);
            return Ok(entries);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id = ?1 ORDER BY entry_type ASC, name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![parent_id.0], row_to_file_entry)?;
        collect_entries(rows)
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
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id = ?1 ORDER BY entry_type ASC, name ASC, id ASC LIMIT ?2 OFFSET ?3",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            params![parent_id.0, limit as i64, offset as i64],
            row_to_file_entry,
        )?;
        collect_entries(rows)
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
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id = ?1 AND hidden = 0 AND system = 0",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![parent_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn count_children(&self, parent_id: &FileEntryId) -> DbResult<u64> {
        if let Some(entries) = self.fjall_entries(None)? {
            return Ok(entries
                .iter()
                .filter(|entry| entry.parent_id.as_ref() == Some(parent_id))
                .count() as u64);
        }
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = ?1",
            params![parent_id.0],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn find_root_entries(&self) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries(None)? {
            entries.retain(|entry| entry.parent_id.is_none());
            sort_entries(&mut entries);
            return Ok(entries);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id IS NULL ORDER BY entry_type ASC, name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_root_entries_page(&self, offset: u64, limit: u32) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id IS NULL ORDER BY entry_type ASC, name ASC, id ASC LIMIT ?1 OFFSET ?2",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![limit as i64, offset as i64], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_root_entries_visible(&self, show_hidden: bool) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_root_entries();
        }
        if let Some(mut entries) = self.find_root_entries().map(Some)? {
            entries.retain(|entry| !entry.hidden && !entry.system);
            return Ok(entries);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id IS NULL AND hidden = 0 AND system = 0",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn count_root_entries(&self) -> DbResult<u64> {
        if let Some(entries) = self.fjall_entries(None)? {
            return Ok(entries
                .iter()
                .filter(|entry| entry.parent_id.is_none())
                .count() as u64);
        }
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn find_child_directories(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.find_children(parent_id).map(Some)? {
            entries.retain(FileEntry::is_directory);
            entries.sort_by(|left, right| left.name.cmp(&right.name));
            return Ok(entries);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE ORDER BY name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![parent_id.0], row_to_file_entry)?;
        collect_entries(rows)
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
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE ORDER BY name ASC LIMIT ?2 OFFSET ?3",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            params![parent_id.0, limit as i64, offset as i64],
            row_to_file_entry,
        )?;
        collect_entries(rows)
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
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE AND hidden = 0 AND system = 0",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![parent_id.0], row_to_file_entry)?;
        collect_entries(rows)
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
        let visibility = if show_hidden {
            ""
        } else {
            " AND hidden = 0 AND system = 0"
        };
        let sql = format!(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE{visibility}"
        );
        Ok(self
            .conn
            .query_row(&sql, params![parent_id.0], |row| row.get::<_, i64>(0))? as u64)
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
        let visibility = if show_hidden {
            ""
        } else {
            " AND hidden = 0 AND system = 0"
        };
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE{visibility} ORDER BY name COLLATE NOCASE ASC, id ASC LIMIT ?2 OFFSET ?3"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            params![parent_id.0, limit as i64, offset as i64],
            row_to_file_entry,
        )?;
        collect_entries(rows)
    }

    pub fn child_directories_need_natural_sort(
        &self,
        parent_id: &FileEntryId,
        show_hidden: bool,
    ) -> DbResult<bool> {
        let visibility = if show_hidden {
            ""
        } else {
            " AND hidden = 0 AND system = 0"
        };
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE{visibility} AND name GLOB '*[0-9]*')"
        );
        Ok(self
            .conn
            .query_row(&sql, params![parent_id.0], |row| row.get(0))?)
    }

    pub fn count_child_directories(&self, parent_id: &FileEntryId) -> DbResult<u64> {
        if let Some(entries) = self.find_child_directories(parent_id).map(Some)? {
            return Ok(entries.len() as u64);
        }
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE",
            params![parent_id.0],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn find_roots(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries(Some(&data_source_id.0))? {
            entries.retain(|entry| entry.parent_id.is_none());
            sort_entries(&mut entries);
            return Ok(entries);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id = ?1 AND parent_id IS NULL ORDER BY entry_type ASC, name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![data_source_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_by_data_source(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries(Some(&data_source_id.0))? {
            sort_entries(&mut entries);
            return Ok(entries);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id = ?1 ORDER BY entry_type ASC, name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![data_source_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_root_directories(&self) -> DbResult<Vec<FileEntry>> {
        if let Some(mut entries) = self.fjall_entries(None)? {
            entries.retain(|entry| {
                entry.is_directory()
                    && (entry.parent_id.is_none() || entry.parent_id.as_ref() == Some(&entry.id))
            });
            entries.sort_by(|left, right| left.name.cmp(&right.name));
            return Ok(entries);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries AS entry
             WHERE (entry.parent_id IS NULL OR entry.parent_id = entry.id OR NOT EXISTS (
                 SELECT 1 FROM file_entries AS parent
                 WHERE parent.id = entry.parent_id AND parent.data_source_id = entry.data_source_id
             ))
               AND entry.entry_type = 'directory' COLLATE NOCASE
             ORDER BY entry.name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_root_directories_visible(&self, show_hidden: bool) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_root_directories();
        }
        if let Some(mut entries) = self.find_root_directories().map(Some)? {
            entries.retain(|entry| !entry.hidden && !entry.system);
            return Ok(entries);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries AS entry
             WHERE (entry.parent_id IS NULL OR entry.parent_id = entry.id OR NOT EXISTS (
                 SELECT 1 FROM file_entries AS parent
                 WHERE parent.id = entry.parent_id AND parent.data_source_id = entry.data_source_id
             ))
               AND entry.entry_type = 'directory' COLLATE NOCASE
               AND entry.hidden = 0 AND entry.system = 0
             ORDER BY entry.name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn has_child_directories(&self, parent_id: &FileEntryId) -> DbResult<bool> {
        if let Some(entries) = self.find_child_directories(parent_id).map(Some)? {
            return Ok(!entries.is_empty());
        }
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE",
            params![parent_id.0],
            |row| row.get(0),
        )?;
        Ok(count > 0)
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
        if let Some(entries) = self.fjall_entries(None)? {
            let wanted = parent_ids
                .iter()
                .map(|id| id.0.as_str())
                .collect::<std::collections::HashSet<_>>();
            let mut result = HashMap::new();
            for entry in entries {
                if entry.is_directory()
                    && entry
                        .parent_id
                        .as_ref()
                        .is_some_and(|id| wanted.contains(id.0.as_str()))
                    && (!visible_only || (!entry.hidden && !entry.system))
                {
                    if let Some(parent) = entry.parent_id {
                        *result.entry(parent.0).or_insert(0) += 1;
                    }
                }
            }
            return Ok(result);
        }
        let visibility = if visible_only {
            " AND hidden = 0 AND system = 0"
        } else {
            ""
        };
        let sql = format!(
            "SELECT parent_id, COUNT(*) FROM file_entries
             WHERE parent_id IN ({}) AND entry_type = 'directory' COLLATE NOCASE{visibility}
             GROUP BY parent_id",
            placeholders(1, parent_ids.len())
        );
        let mut statement = self.conn.prepare(&sql)?;
        let parameters = parent_ids
            .iter()
            .map(|id| id.0.as_str())
            .collect::<Vec<_>>();
        let rows = statement.query_map(rusqlite::params_from_iter(parameters.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut result = HashMap::new();
        for row in rows {
            let (parent_id, count) = row?;
            result.insert(parent_id, count);
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
