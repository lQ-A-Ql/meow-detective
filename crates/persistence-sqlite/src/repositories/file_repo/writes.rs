use crate::connection::DbResult;
use domain::{EntryType, FileEntry};
use rusqlite::params;

use super::FileRepo;

impl FileRepo<'_> {
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
            "INSERT OR IGNORE INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted, read_only, archive, unix_mode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
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
                entry.read_only as i32,
                entry.archive as i32,
                entry.unix_mode,
            ])?;
        }
        Ok(())
    }
}
