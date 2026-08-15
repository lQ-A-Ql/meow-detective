use rusqlite::Connection;

mod browse;
mod lookup;
mod mapping;
mod writes;

pub use mapping::file_encryption_status_from_row;

pub(super) const FILE_ENTRY_COLUMNS: &str = "id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted, read_only, archive, unix_mode";

pub struct FileRepo<'a> {
    pub(super) conn: &'a Connection,
}

impl<'a> FileRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/file_repo.rs"]
mod tests;
