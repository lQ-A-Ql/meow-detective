use crate::connection::DbResult;
use domain::{FileEntry, FileEntryId};
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
}
