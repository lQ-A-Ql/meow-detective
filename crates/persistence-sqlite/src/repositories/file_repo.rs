use crate::connection::DbResult;
use crate::util::parse_opt_datetime;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use rusqlite::{params, Connection};

pub struct FileRepo<'a> {
    conn: &'a Connection,
}

impl<'a> FileRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_batch(&self, entries: &[FileEntry]) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                    entry.created_at.map(|dt| dt.to_rfc3339()),
                    entry.modified_at.map(|dt| dt.to_rfc3339()),
                    entry.accessed_at.map(|dt| dt.to_rfc3339()),
                    entry.changed_at.map(|dt| dt.to_rfc3339()),
                    entry.hash_sha256,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn find_children(&self, parent_id: &FileEntryId) -> DbResult<Vec<FileEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256
             FROM file_entries WHERE parent_id = ?1 ORDER BY entry_type ASC, name ASC",
        )?;
        let rows = stmt.query_map(params![parent_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_roots(&self, data_source_id: &DataSourceId) -> DbResult<Vec<FileEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256
             FROM file_entries WHERE data_source_id = ?1 AND parent_id IS NULL ORDER BY entry_type ASC, name ASC",
        )?;
        let rows = stmt.query_map(params![data_source_id.0], row_to_file_entry)?;
        collect_entries(rows)
    }

    pub fn find_by_path_prefix(&self, data_source_id: &DataSourceId, prefix: &str) -> DbResult<Vec<FileEntry>> {
        let pattern = format!("{}%", prefix);
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256
             FROM file_entries WHERE data_source_id = ?1 AND path LIKE ?2 ORDER BY path ASC",
        )?;
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

    pub fn find_by_id(&self, id: &FileEntryId) -> DbResult<Option<FileEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256
             FROM file_entries WHERE id = ?1",
        )?;
        let result = stmt.query_row(params![id.0], row_to_file_entry);
        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

fn row_to_file_entry(row: &rusqlite::Row) -> rusqlite::Result<FileEntry> {
    let entry_type_str: String = row.get(5)?;
    Ok(FileEntry {
        id: FileEntryId(row.get::<_, String>(0)?),
        parent_id: row.get::<_, Option<String>>(1)?.map(FileEntryId),
        data_source_id: DataSourceId(row.get::<_, String>(2)?),
        path: row.get(3)?,
        name: row.get(4)?,
        entry_type: if entry_type_str == "directory" {
            EntryType::Directory
        } else {
            EntryType::File
        },
        size: row.get(6)?,
        ext: row.get(7)?,
        deleted: row.get::<_, i32>(8)? != 0,
        created_at: row.get::<_, Option<String>>(9)?.and_then(|s| parse_opt_datetime(&s)),
        modified_at: row.get::<_, Option<String>>(10)?.and_then(|s| parse_opt_datetime(&s)),
        accessed_at: row.get::<_, Option<String>>(11)?.and_then(|s| parse_opt_datetime(&s)),
        changed_at: row.get::<_, Option<String>>(12)?.and_then(|s| parse_opt_datetime(&s)),
        hash_sha256: row.get(13)?,
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
