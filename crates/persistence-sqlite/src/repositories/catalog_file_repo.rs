use crate::connection::{DbError, DbResult};
use crate::repositories::fingerprint_repo::ForensicFingerprintRepo;
use domain::{EntryType, FileEntry};
use rusqlite::{params, Connection, OptionalExtension};

pub struct CatalogFileRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CatalogFileRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_batch_with_partition_index_in_transaction(
        &self,
        entries: &[FileEntry],
        partition_index: usize,
    ) -> DbResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let partition_index = i64::try_from(partition_index)
            .map_err(|_| DbError::System("Partition index exceeds SQLite range".to_string()))?;
        let mut statement = self.conn.prepare_cached(
            "INSERT INTO file_entries (
                id, parent_id, data_source_id, path, name, entry_type,
                size, ext, deleted, hidden, system, read_only, archive, unix_mode, encrypted, created_at, modified_at,
                accessed_at, changed_at, hash_sha256, partition_index
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
             )",
        )?;
        let fingerprint_repo = ForensicFingerprintRepo::new(self.conn);
        let fingerprints_available = fingerprint_repo.is_available()?;
        let case_id = if fingerprints_available {
            case_id_for_source(self.conn, entries[0].data_source_id.0.as_str())?
        } else {
            None
        };
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
                entry.read_only as i32,
                entry.archive as i32,
                entry.unix_mode,
                entry.encrypted as i32,
                entry.created_at.map(|value| value.to_rfc3339()),
                entry.modified_at.map(|value| value.to_rfc3339()),
                entry.accessed_at.map(|value| value.to_rfc3339()),
                entry.changed_at.map(|value| value.to_rfc3339()),
                entry.hash_sha256,
                partition_index,
            ])?;
            if fingerprints_available {
                fingerprint_repo.upsert_in_transaction(
                    &domain::ForensicFingerprint::for_file_entry(entry, case_id.as_deref()),
                )?;
            }
        }
        Ok(())
    }

    pub fn update_root_in_transaction(&self, entry: &FileEntry) -> DbResult<()> {
        self.conn.execute(
            "UPDATE file_entries
             SET path = ?1, name = ?2, entry_type = 'directory', size = NULL,
                 ext = NULL, deleted = ?3, hidden = ?4, system = ?5, encrypted = ?6,
                 read_only = ?7, archive = ?8, unix_mode = ?9,
                 created_at = ?10, modified_at = ?11, accessed_at = ?12,
                 changed_at = ?13, hash_sha256 = NULL
             WHERE id = ?14",
            params![
                entry.path,
                entry.name,
                entry.deleted as i32,
                entry.hidden as i32,
                entry.system as i32,
                entry.encrypted as i32,
                entry.read_only as i32,
                entry.archive as i32,
                entry.unix_mode,
                entry.created_at.map(|value| value.to_rfc3339()),
                entry.modified_at.map(|value| value.to_rfc3339()),
                entry.accessed_at.map(|value| value.to_rfc3339()),
                entry.changed_at.map(|value| value.to_rfc3339()),
                entry.id.0,
            ],
        )?;
        ForensicFingerprintRepo::new(self.conn).upsert_if_available(
            &domain::ForensicFingerprint::for_file_entry(
                entry,
                case_id_for_source(self.conn, entry.data_source_id.0.as_str())?.as_deref(),
            ),
        )?;
        Ok(())
    }
}

fn case_id_for_source(conn: &Connection, source_id: &str) -> DbResult<Option<String>> {
    conn.query_row(
        "SELECT case_id FROM data_sources WHERE id = ?1",
        [source_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}
