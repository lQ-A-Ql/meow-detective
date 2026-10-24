use crate::connection::DbResult;
use crate::repositories::fingerprint_repo::ForensicFingerprintRepo;
use domain::{EntryType, FileEntry};
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

use super::FileRepo;

impl FileRepo<'_> {
    pub fn remove_partition_placeholder_root(
        &self,
        data_source_id: &str,
        path_pattern: &str,
    ) -> DbResult<usize> {
        Ok(self.conn.execute(
            "DELETE FROM file_entries WHERE data_source_id = ?1 AND parent_id IS NULL AND path GLOB ?2",
            params![data_source_id, path_pattern],
        )?)
    }

    pub fn update_root_metadata(&self, entry: &FileEntry) -> DbResult<()> {
        self.conn.execute(
            "UPDATE file_entries SET path = ?1, name = ?2, created_at = ?3, modified_at = ?4,
             accessed_at = ?5, hidden = ?6, system = ?7 WHERE id = ?8",
            params![
                entry.path,
                entry.name,
                entry.created_at.map(|value| value.to_rfc3339()),
                entry.modified_at.map(|value| value.to_rfc3339()),
                entry.accessed_at.map(|value| value.to_rfc3339()),
                entry.hidden as i32,
                entry.system as i32,
                entry.id.0,
            ],
        )?;
        Ok(())
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
            "INSERT OR IGNORE INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted, read_only, archive, unix_mode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        )?;
        let fingerprint_repo = ForensicFingerprintRepo::new(self.conn);
        let fingerprints_available = fingerprint_repo.is_available()?;
        let case_ids = fingerprints_available
            .then(|| case_ids_by_source(self.conn, entries))
            .transpose()?;
        for entry in entries {
            let inserted = statement.execute(params![
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
            if fingerprints_available && inserted != 0 {
                let case_id = case_ids
                    .as_ref()
                    .and_then(|ids| ids.get(&entry.data_source_id.0))
                    .map(String::as_str);
                fingerprint_repo.upsert_in_transaction(
                    &domain::ForensicFingerprint::for_file_entry(entry, case_id),
                )?;
            }
        }
        Ok(())
    }
}

fn case_ids_by_source(
    conn: &rusqlite::Connection,
    entries: &[FileEntry],
) -> DbResult<HashMap<String, String>> {
    let mut ids = HashMap::new();
    let mut query = conn.prepare_cached("SELECT case_id FROM data_sources WHERE id = ?1")?;
    for data_source_id in entries.iter().map(|entry| &entry.data_source_id.0) {
        if ids.contains_key(data_source_id) {
            continue;
        }
        if let Some(case_id) = query
            .query_row([data_source_id], |row| row.get::<_, String>(0))
            .optional()?
        {
            ids.insert(data_source_id.clone(), case_id);
        }
    }
    Ok(ids)
}
