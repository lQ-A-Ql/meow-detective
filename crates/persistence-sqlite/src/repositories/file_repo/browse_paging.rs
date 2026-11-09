use crate::connection::DbResult;
use domain::{FileEntry, FileEntryId};
use rusqlite::params;

use super::{mapping::collect_entries, mapping::row_to_file_entry, FileRepo, FILE_ENTRY_COLUMNS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSortKey {
    Name,
    Size,
    ModifiedAt,
    Ext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSortDirection {
    Asc,
    Desc,
}

impl FileRepo<'_> {
    pub fn count_entries_visible(
        &self,
        parent_id: Option<&FileEntryId>,
        show_hidden: bool,
    ) -> DbResult<u64> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let entries = match parent_id {
                Some(parent_id) => self.find_children_visible(parent_id, show_hidden)?,
                None => self.find_root_entries_visible(show_hidden)?,
            };
            return Ok(entries.len() as u64);
        }
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
        sort_key: FileSortKey,
    ) -> DbResult<bool> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let entries = match parent_id {
                Some(parent_id) => self.find_children_visible(parent_id, show_hidden)?,
                None => self.find_root_entries_visible(show_hidden)?,
            };
            return Ok(entries.iter().any(|entry| match sort_key {
                FileSortKey::Ext => {
                    contains_ascii_digit(&entry.name)
                        || entry.ext.as_deref().is_some_and(contains_ascii_digit)
                }
                FileSortKey::Name => contains_ascii_digit(&entry.name),
                FileSortKey::Size | FileSortKey::ModifiedAt => false,
            }));
        }
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
            FileSortKey::Ext => "(name GLOB '*[0-9]*' OR ext GLOB '*[0-9]*')",
            FileSortKey::Name => "name GLOB '*[0-9]*'",
            FileSortKey::Size | FileSortKey::ModifiedAt => return Ok(false),
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
        sort_key: FileSortKey,
        sort_direction: FileSortDirection,
    ) -> DbResult<Vec<FileEntry>> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let mut entries = match parent_id {
                Some(parent_id) => self.find_children(parent_id)?,
                None => self.find_root_entries()?,
            };
            if !show_hidden {
                entries.retain(|entry| !entry.hidden && !entry.system);
            }
            sort_native_entries(&mut entries, sort_key, sort_direction);
            return Ok(entries
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect());
        }
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
            FileSortKey::Name => "name COLLATE NOCASE",
            FileSortKey::Size => "COALESCE(size, 0)",
            FileSortKey::ModifiedAt => "modified_at",
            FileSortKey::Ext => "COALESCE(ext, '') COLLATE NOCASE",
        };
        let direction = match sort_direction {
            FileSortDirection::Asc => "ASC",
            FileSortDirection::Desc => "DESC",
        };
        let raw_key_tie = if matches!(sort_key, FileSortKey::Ext) {
            format!("COALESCE(ext, '') {direction},")
        } else {
            String::new()
        };
        let null_order = if matches!(sort_key, FileSortKey::ModifiedAt) {
            match sort_direction {
                FileSortDirection::Asc => "(modified_at IS NOT NULL) ASC,",
                FileSortDirection::Desc => "(modified_at IS NOT NULL) DESC,",
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

fn contains_ascii_digit(value: &str) -> bool {
    value.bytes().any(|byte| byte.is_ascii_digit())
}

fn sort_native_entries(
    entries: &mut [FileEntry],
    sort_key: FileSortKey,
    sort_direction: FileSortDirection,
) {
    entries.sort_by(|left, right| {
        right
            .is_directory()
            .cmp(&left.is_directory())
            .then_with(|| visibility_rank(left).cmp(&visibility_rank(right)))
            .then_with(|| {
                let ordering = match sort_key {
                    FileSortKey::Name => left
                        .name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase()),
                    FileSortKey::Size => left
                        .size
                        .unwrap_or_default()
                        .cmp(&right.size.unwrap_or_default()),
                    FileSortKey::ModifiedAt => left.modified_at.cmp(&right.modified_at),
                    FileSortKey::Ext => left
                        .ext
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .cmp(
                            &right
                                .ext
                                .as_deref()
                                .unwrap_or_default()
                                .to_ascii_lowercase(),
                        ),
                };
                match sort_direction {
                    FileSortDirection::Asc => ordering,
                    FileSortDirection::Desc => ordering.reverse(),
                }
            })
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
}

fn visibility_rank(entry: &FileEntry) -> u8 {
    match (entry.deleted, entry.hidden || entry.system) {
        (false, false) => 0,
        (false, true) => 1,
        (true, false) => 2,
        (true, true) => 3,
    }
}
