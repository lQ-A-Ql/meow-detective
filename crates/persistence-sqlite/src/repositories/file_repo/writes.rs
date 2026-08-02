use crate::connection::DbResult;
use domain::{EntryType, FileEntry};
use rusqlite::{params, Connection};

use super::FileRepo;

impl FileRepo<'_> {
    pub fn insert_batch_transactional(&self, entries: &[FileEntry]) -> DbResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        self.insert_batch(entries)
    }

    pub fn insert_batch(&self, entries: &[FileEntry]) -> DbResult<()> {
        let transaction = self.conn.unchecked_transaction()?;
        FileRepo::new(&transaction).insert_batch_unchecked(entries)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_batch_unchecked(&self, entries: &[FileEntry]) -> DbResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut statement = self.conn.prepare_cached(
            "INSERT OR IGNORE INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        )?;
        for entry in entries {
            statement.execute(params![
                entry.id.0,
                entry.parent_id.as_ref().map(|parent| &parent.0),
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
                entry.created_at.map(|value| value.to_rfc3339()),
                entry.modified_at.map(|value| value.to_rfc3339()),
                entry.accessed_at.map(|value| value.to_rfc3339()),
                entry.changed_at.map(|value| value.to_rfc3339()),
                entry.hash_sha256,
                entry.encrypted as i32,
            ])?;
        }
        Ok(())
    }

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
