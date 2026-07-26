use crate::connection::DbResult;
use crate::sql_builder::placeholders;
use crate::util::parse_opt_datetime;
use domain::{DataSourceId, EntryType, FileEncryptionStatus, FileEntry, FileEntryId};
use rusqlite::{params, Connection};
use std::collections::HashMap;

const FILE_ENTRY_COLUMNS: &str = "id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted";

pub struct FileRepo<'a> {
    conn: &'a Connection,
}

impl<'a> FileRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert multiple file entries in a single transaction.
    /// This is more efficient than calling insert_batch multiple times.
    pub fn insert_batch_transactional(&self, entries: &[FileEntry]) -> DbResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        self.insert_batch(entries)
    }

    pub fn insert_batch(&self, entries: &[FileEntry]) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let repo = FileRepo::new(&tx);
            repo.insert_batch_unchecked(entries)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Insert file entries using the current connection/transaction.
    pub fn insert_batch_unchecked(&self, entries: &[FileEntry]) -> DbResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut stmt = self.conn.prepare_cached(
            // INSERT OR IGNORE is used here so that when multiple data sources
            // share overlapping MFT record numbers (e.g. importing several
            // partitions from the same E01 or logical image), only the first
            // inserted row for a given id wins and later overlaps are silently
            // skipped instead of rolling back the entire batch transaction.
            "INSERT OR IGNORE INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        )?;
        for entry in entries {
            stmt.execute(params![
                entry.id.0,
                entry.parent_id.as_ref().map(|p| &p.0),
                entry.data_source_id.0,
                entry.path,
                entry.name,
                match entry.entry_type {
                    EntryType::File => "file",
                    EntryType::Directory => "directory",
                },
                entry.size,
                entry.ext,
                entry.deleted as i32,
                entry.hidden as i32,
                entry.system as i32,
                entry.created_at.map(|dt| dt.to_rfc3339()),
                entry.modified_at.map(|dt| dt.to_rfc3339()),
                entry.accessed_at.map(|dt| dt.to_rfc3339()),
                entry.changed_at.map(|dt| dt.to_rfc3339()),
                entry.hash_sha256,
                entry.encrypted as i32,
            ])?;
        }
        Ok(())
    }

    pub fn find_children(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id = ?1 ORDER BY entry_type ASC, name ASC",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![parent_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_children_page(
        &self,
        parent_id: &FileEntryId,
        offset: u64,
        limit: u32,
    ) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id = ?1 ORDER BY entry_type ASC, name ASC LIMIT ?2 OFFSET ?3",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![parent_id.0, limit as i64, offset as i64],
            row_to_file_entry,
        )?;
        collect_entries(rows)
    }

    pub fn find_children_page_visible(
        &self,
        parent_id: &FileEntryId,
        offset: u64,
        limit: u32,
        show_hidden: bool,
    ) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_children_page(parent_id, offset, limit);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id = ?1 AND hidden = 0 AND system = 0
             ORDER BY entry_type ASC, name ASC LIMIT ?2 OFFSET ?3",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
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
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![parent_id.0], row_to_file_entry)?;
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

    pub fn count_children_visible(
        &self,
        parent_id: &FileEntryId,
        show_hidden: bool,
    ) -> DbResult<u64> {
        if show_hidden {
            return self.count_children(parent_id);
        }
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = ?1 AND hidden = 0 AND system = 0",
            params![parent_id.0],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn find_root_entries(&self) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id IS NULL ORDER BY entry_type ASC, name ASC",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_root_entries_page(&self, offset: u64, limit: u32) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id IS NULL ORDER BY entry_type ASC, name ASC LIMIT ?1 OFFSET ?2",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_root_entries_page_visible(
        &self,
        offset: u64,
        limit: u32,
        show_hidden: bool,
    ) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_root_entries_page(offset, limit);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id IS NULL AND hidden = 0 AND system = 0
             ORDER BY entry_type ASC, name ASC LIMIT ?1 OFFSET ?2",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], row_to_file_entry)?;
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
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_file_entry)?;
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

    pub fn count_root_entries_visible(&self, show_hidden: bool) -> DbResult<u64> {
        if show_hidden {
            return self.count_root_entries();
        }
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id IS NULL AND hidden = 0 AND system = 0",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn find_child_directories(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE ORDER BY name ASC",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![parent_id.0], row_to_file_entry)?;
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
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![parent_id.0, limit as i64, offset as i64],
            row_to_file_entry,
        )?;
        collect_entries(rows)
    }

    pub fn find_child_directories_page_visible(
        &self,
        parent_id: &FileEntryId,
        offset: u64,
        limit: u32,
        show_hidden: bool,
    ) -> DbResult<Vec<FileEntry>> {
        if show_hidden {
            return self.find_child_directories_page(parent_id, offset, limit);
        }
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries
             WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE AND hidden = 0 AND system = 0
             ORDER BY name ASC LIMIT ?2 OFFSET ?3",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
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
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![parent_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn count_child_directories(&self, parent_id: &FileEntryId) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE",
            params![parent_id.0],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn count_child_directories_visible(
        &self,
        parent_id: &FileEntryId,
        show_hidden: bool,
    ) -> DbResult<u64> {
        if show_hidden {
            return self.count_child_directories(parent_id);
        }
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = ?1 AND entry_type = 'directory' COLLATE NOCASE AND hidden = 0 AND system = 0",
            params![parent_id.0],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn find_roots(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id = ?1 AND parent_id IS NULL ORDER BY entry_type ASC, name ASC",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![data_source_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_by_data_source(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id = ?1 ORDER BY entry_type ASC, name ASC",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![data_source_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_root_directories(&self) -> DbResult<Vec<FileEntry>> {
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE parent_id IS NULL AND entry_type = 'directory' COLLATE NOCASE ORDER BY name ASC",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_file_entry)?;
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
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_file_entry)?;
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
        if parent_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let sql = format!(
            "SELECT parent_id, COUNT(*) FROM file_entries
             WHERE parent_id IN ({}) AND entry_type = 'directory' COLLATE NOCASE
             GROUP BY parent_id",
            placeholders(1, parent_ids.len())
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&str> = parent_ids.iter().map(|id| id.0.as_str()).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut result = HashMap::new();
        for row in rows {
            let (pid, count) = row?;
            result.insert(pid, count);
        }
        Ok(result)
    }

    pub fn count_child_directories_batch_visible(
        &self,
        parent_ids: &[&FileEntryId],
        show_hidden: bool,
    ) -> DbResult<HashMap<String, i64>> {
        if show_hidden {
            return self.count_child_directories_batch(parent_ids);
        }
        if parent_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let sql = format!(
            "SELECT parent_id, COUNT(*) FROM file_entries
             WHERE parent_id IN ({}) AND entry_type = 'directory' COLLATE NOCASE AND hidden = 0 AND system = 0
             GROUP BY parent_id",
            placeholders(1, parent_ids.len())
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&str> = parent_ids.iter().map(|id| id.0.as_str()).collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut result = HashMap::new();
        for row in rows {
            let (pid, count) = row?;
            result.insert(pid, count);
        }
        Ok(result)
    }

    pub fn find_by_path_prefix(
        &self,
        data_source_id: &DataSourceId,
        prefix: &str,
    ) -> DbResult<Vec<FileEntry>> {
        let pattern = format!("{}%", escape_like_literal(prefix));
        let sql = format!(
            "SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE data_source_id = ?1 AND path LIKE ?2 ESCAPE '\\' ORDER BY path ASC",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![data_source_id.0, pattern], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn count_by_data_source(&self, data_source_id: &DataSourceId) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1",
            params![data_source_id.0],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn count_all(&self) -> DbResult<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn find_by_id(&self, id: &FileEntryId) -> DbResult<Option<FileEntry>> {
        let sql = format!("SELECT {FILE_ENTRY_COLUMNS} FROM file_entries WHERE id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let result = stmt.query_row(params![id.0], row_to_file_entry);
        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn find_encryption_status(
        &self,
        id: &FileEntryId,
    ) -> DbResult<Option<FileEncryptionStatus>> {
        let result = self.conn.query_row(
            "SELECT encrypted FROM file_entries WHERE id = ?1",
            params![id.0],
            |row| file_encryption_status_from_row(row, 0),
        );
        match result {
            Ok(status) => Ok(Some(status)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn find_partition_index_by_id(&self, id: &FileEntryId) -> DbResult<Option<usize>> {
        let result = self.conn.query_row(
            "SELECT partition_index FROM file_entries WHERE id = ?1",
            params![id.0],
            |row| row.get::<_, Option<i64>>(0),
        );
        match result {
            Ok(Some(index)) => usize::try_from(index).map(Some).map_err(|_| {
                crate::connection::DbError::System(format!(
                    "File entry '{}' has invalid partition index {}",
                    id.0, index
                ))
            }),
            Ok(None) | Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn set_partition_index_by_id(
        &self,
        id: &FileEntryId,
        partition_index: usize,
    ) -> DbResult<()> {
        let partition_index = i64::try_from(partition_index).map_err(|_| {
            crate::connection::DbError::System(format!(
                "Partition index is too large for SQLite: {partition_index}"
            ))
        })?;
        self.conn.execute(
            "UPDATE file_entries SET partition_index = ?1 WHERE id = ?2",
            params![partition_index, id.0],
        )?;
        Ok(())
    }

    pub fn assign_partition_index_to_subtree(
        &self,
        root_id: &FileEntryId,
        partition_index: usize,
    ) -> DbResult<usize> {
        let partition_index = i64::try_from(partition_index).map_err(|_| {
            crate::connection::DbError::System(format!(
                "Partition index is too large for SQLite: {partition_index}"
            ))
        })?;
        let updated = self.conn.execute(
            "WITH RECURSIVE subtree(id, data_source_id) AS (
                 SELECT id, data_source_id
                 FROM file_entries
                 WHERE id = ?1

                 UNION

                 SELECT child.id, child.data_source_id
                 FROM file_entries AS child
                 JOIN subtree AS parent
                   ON child.parent_id = parent.id
                  AND child.data_source_id = parent.data_source_id
             )
             UPDATE file_entries
             SET partition_index = ?2
             WHERE id IN (SELECT id FROM subtree)",
            params![root_id.0, partition_index],
        )?;
        Ok(updated)
    }

    pub fn find_data_source_location(
        &self,
        data_source_id: &DataSourceId,
    ) -> DbResult<Option<(String, String)>> {
        let result = self.conn.query_row(
            "SELECT kind, source_path FROM data_sources WHERE id = ?1",
            params![data_source_id.0],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok(location) => Ok(Some(location)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

fn escape_like_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn row_to_file_entry(row: &rusqlite::Row) -> rusqlite::Result<FileEntry> {
    let entry_type_str: String = row.get(5)?;
    let encryption_status = file_encryption_status_from_row(row, 16)?;
    Ok(FileEntry {
        id: FileEntryId(row.get::<_, String>(0)?),
        parent_id: row.get::<_, Option<String>>(1)?.map(FileEntryId),
        data_source_id: DataSourceId(row.get::<_, String>(2)?),
        path: row.get(3)?,
        name: row.get(4)?,
        entry_type: if entry_type_str.eq_ignore_ascii_case("directory") {
            EntryType::Directory
        } else {
            EntryType::File
        },
        size: row.get(6)?,
        ext: row.get(7)?,
        deleted: row.get::<_, i32>(8)? != 0,
        hidden: row.get::<_, i32>(9)? != 0,
        system: row.get::<_, i32>(10)? != 0,
        encrypted: encryption_status.blocks_content(),
        created_at: row
            .get::<_, Option<String>>(11)?
            .and_then(|s| parse_opt_datetime(&s)),
        modified_at: row
            .get::<_, Option<String>>(12)?
            .and_then(|s| parse_opt_datetime(&s)),
        accessed_at: row
            .get::<_, Option<String>>(13)?
            .and_then(|s| parse_opt_datetime(&s)),
        changed_at: row
            .get::<_, Option<String>>(14)?
            .and_then(|s| parse_opt_datetime(&s)),
        hash_sha256: row.get(15)?,
    })
}

pub fn file_encryption_status_from_row(
    row: &rusqlite::Row<'_>,
    column_index: usize,
) -> rusqlite::Result<FileEncryptionStatus> {
    let value = row.get::<_, Option<i64>>(column_index)?;
    FileEncryptionStatus::from_database_value(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column_index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn collect_entries(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row) -> rusqlite::Result<FileEntry>>,
) -> DbResult<Vec<FileEntry>> {
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

impl FileRepo<'_> {
    /// Insert a single file entry row with explicit column values. Uses
    /// `INSERT OR IGNORE` so overlapping ids are silently skipped.
    ///
    /// Returns `true` when a row was actually inserted.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_file_entry_row(
        conn: &Connection,
        id: &str,
        parent_id: Option<&str>,
        data_source_id: &str,
        name: &str,
        entry_type: &str,
        size: Option<i64>,
        ext: Option<&str>,
        deleted: bool,
        hidden: bool,
        system: bool,
        encrypted: bool,
        created_at: Option<&str>,
        modified_at: Option<&str>,
        accessed_at: Option<&str>,
        changed_at: Option<&str>,
        hash_sha256: Option<&str>,
        partition_index: Option<i64>,
    ) -> DbResult<bool> {
        let changed = conn.execute(
            "INSERT OR IGNORE INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type,
              size, ext, deleted, hidden, system, encrypted, created_at, modified_at,
              accessed_at, changed_at, hash_sha256, partition_index)
             VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                id,
                parent_id,
                data_source_id,
                name,
                entry_type,
                size,
                ext,
                deleted as i32,
                hidden as i32,
                system as i32,
                encrypted as i32,
                created_at,
                modified_at,
                accessed_at,
                changed_at,
                hash_sha256,
                partition_index,
            ],
        )?;
        Ok(changed > 0)
    }

    /// Update the path of a single `file_entries` row.
    pub fn update_file_entry_path(
        conn: &Connection,
        entry_id: &str,
        data_source_id: &str,
        path: &str,
    ) -> DbResult<()> {
        conn.execute(
            "UPDATE file_entries SET path = ?1 WHERE id = ?2 AND data_source_id = ?3",
            params![path, entry_id, data_source_id],
        )?;
        Ok(())
    }

    /// Update the path and parent_id of a single `file_entries` row.
    pub fn update_file_entry_parent_path(
        conn: &Connection,
        entry_id: &str,
        data_source_id: &str,
        parent_id: Option<&str>,
        path: &str,
    ) -> DbResult<()> {
        conn.execute(
            "UPDATE file_entries SET path = ?1, parent_id = ?2
             WHERE id = ?3 AND data_source_id = ?4",
            params![path, parent_id, entry_id, data_source_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/file_repo.rs"]
mod tests;
