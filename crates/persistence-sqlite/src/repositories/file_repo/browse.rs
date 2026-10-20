use std::collections::HashMap;

use crate::{connection::DbResult, sql_builder::placeholders};
use domain::{DataSourceId, FileEntry, FileEntryId};
use rusqlite::params;
use transport::commands::{FileSortDirectionDto, FileSortKeyDto};

use super::{mapping::collect_entries, mapping::row_to_file_entry, FileRepo, FILE_ENTRY_COLUMNS};

impl FileRepo<'_> {
    pub fn count_entries_visible(
        &self,
        parent_id: Option<&FileEntryId>,
        show_hidden: bool,
    ) -> DbResult<u64> {
        let parent = if parent_id.is_some() {
            "parent_id = ?1"
        } else {
            "parent_id IS NULL"
        };
        let visibility = if show_hidden {
            ""
        } else {
            " AND hidden = 0 AND system = 0"
        };
        let sql = format!("SELECT COUNT(*) FROM file_entries WHERE {parent}{visibility}");
        let count = match parent_id {
            Some(parent_id) => self
                .conn
                .query_row(&sql, params![parent_id.0], |row| row.get::<_, i64>(0))?,
            None => self.conn.query_row(&sql, [], |row| row.get::<_, i64>(0))?,
        };
        Ok(count as u64)
    }

    pub fn entries_need_natural_sort(
        &self,
        parent_id: Option<&FileEntryId>,
        show_hidden: bool,
        sort_key: FileSortKeyDto,
    ) -> DbResult<bool> {
        let parent = if parent_id.is_some() {
            "parent_id = ?1"
        } else {
            "parent_id IS NULL"
        };
        let visibility = if show_hidden {
            ""
        } else {
            " AND hidden = 0 AND system = 0"
        };
        let field = match sort_key {
            FileSortKeyDto::Ext => "(name GLOB '*[0-9]*' OR ext GLOB '*[0-9]*')",
            FileSortKeyDto::Name => "name GLOB '*[0-9]*'",
            FileSortKeyDto::Size | FileSortKeyDto::ModifiedAt => return Ok(false),
        };
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM file_entries WHERE {parent}{visibility} AND {field} GLOB '*[0-9]*')"
        );
        let value = match parent_id {
            Some(parent_id) => self
                .conn
                .query_row(&sql, params![parent_id.0], |row| row.get(0))?,
            None => self.conn.query_row(&sql, [], |row| row.get(0))?,
        };
        Ok(value)
    }

    pub fn find_entries_page_visible_ordered(
        &self,
        parent_id: Option<&FileEntryId>,
        offset: u64,
        limit: u32,
        show_hidden: bool,
        sort_key: FileSortKeyDto,
        sort_direction: FileSortDirectionDto,
    ) -> DbResult<Vec<FileEntry>> {
        let parent = if parent_id.is_some() {
            "parent_id = ?1"
        } else {
            "parent_id IS NULL"
        };
        let visibility = if show_hidden {
            ""
        } else {
            " AND hidden = 0 AND system = 0"
        };
        let key = match sort_key {
            FileSortKeyDto::Name => "name COLLATE NOCASE",
            FileSortKeyDto::Size => "COALESCE(size, 0)",
            FileSortKeyDto::ModifiedAt => "modified_at",
            FileSortKeyDto::Ext => "COALESCE(ext, '') COLLATE NOCASE",
        };
        let direction = match sort_direction {
            FileSortDirectionDto::Asc => "ASC",
            FileSortDirectionDto::Desc => "DESC",
        };
        let raw_key_tie = if matches!(sort_key, FileSortKeyDto::Ext) {
            format!("COALESCE(ext, '') {direction},")
        } else {
            String::new()
        };
        let null_order = if matches!(sort_key, FileSortKeyDto::ModifiedAt) {
            match sort_direction {
                FileSortDirectionDto::Asc => "(modified_at IS NOT NULL) ASC,",
                FileSortDirectionDto::Desc => "(modified_at IS NOT NULL) DESC,",
            }
        } else {
            ""
        };
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE {parent}{visibility}
             ORDER BY CASE WHEN entry_type = 'directory' COLLATE NOCASE THEN 0 ELSE 1 END ASC,
                      CASE WHEN hidden = 0 AND system = 0 AND deleted = 0 THEN 0
                           WHEN deleted = 0 THEN 1 WHEN hidden = 0 AND system = 0 THEN 2 ELSE 3 END ASC,
                      {null_order} {key} {direction}, {raw_key_tie} name COLLATE NOCASE ASC, name ASC, id ASC
             LIMIT ?{limit_param} OFFSET ?{offset_param}",
            limit_param = if parent_id.is_some() { 2 } else { 1 },
            offset_param = if parent_id.is_some() { 3 } else { 2 },
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = match parent_id {
            Some(parent_id) => statement.query_map(
                params![parent_id.0, limit as i64, offset as i64],
                row_to_file_entry,
            )?,
            None => statement.query_map(params![limit as i64, offset as i64], row_to_file_entry)?,
        };
        collect_entries(rows)
    }
    pub fn find_mount_children_for_partition(
        &self,
        parent_id: &FileEntryId,
        data_source_id: &DataSourceId,
        partition_index: usize,
    ) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id = ?1 AND data_source_id = ?2 AND partition_index = ?3
               AND deleted = 0
             ORDER BY entry_type ASC, name COLLATE NOCASE ASC, id ASC",
        );
        let mut statement = self.conn.prepare_cached(&sql)?;
        let rows = statement.query_map(
            params![parent_id.0, data_source_id.0, partition_index as u64],
            row_to_file_entry,
        )?;
        collect_entries(rows)
    }

    pub fn find_root_for_partition(
        &self,
        data_source_id: &DataSourceId,
        partition_index: usize,
    ) -> DbResult<Option<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id IS NULL AND data_source_id = ?1 AND partition_index = ?2
               AND deleted = 0
             ORDER BY id ASC LIMIT 1",
        );
        let result = self.conn.query_row(
            &sql,
            params![data_source_id.0, partition_index as u64],
            row_to_file_entry,
        );
        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn find_children_page_for_partition(
        &self,
        parent_id: &FileEntryId,
        data_source_id: &DataSourceId,
        partition_index: usize,
        offset: u64,
        limit: u32,
    ) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id = ?1 AND data_source_id = ?2 AND partition_index = ?3
               AND deleted = 0
             ORDER BY entry_type ASC, name ASC LIMIT ?4 OFFSET ?5",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            params![
                parent_id.0,
                data_source_id.0,
                partition_index as u64,
                limit as u64,
                offset
            ],
            row_to_file_entry,
        )?;
        collect_entries(rows)
    }

    pub fn find_children(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
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
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id = ?1 AND hidden = 0 AND system = 0",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![parent_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn count_children(&self, parent_id: &FileEntryId) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = ?1",
            params![parent_id.0],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn find_root_entries(&self) -> DbResult<Vec<FileEntry>> {
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
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id IS NULL AND hidden = 0 AND system = 0",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn count_root_entries(&self) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn find_child_directories(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
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
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE",
            params![parent_id.0],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn find_roots(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id = ?1 AND parent_id IS NULL ORDER BY entry_type ASC, name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![data_source_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_by_data_source(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id = ?1 ORDER BY entry_type ASC, name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![data_source_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_root_directories(&self) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id IS NULL AND entry_type = 'directory' COLLATE NOCASE ORDER BY name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_root_directories_visible(&self, show_hidden: bool) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_root_directories();
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id IS NULL AND entry_type = 'directory' COLLATE NOCASE AND hidden = 0 AND system = 0
             ORDER BY name ASC",
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn has_child_directories(&self, parent_id: &FileEntryId) -> DbResult<bool> {
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
