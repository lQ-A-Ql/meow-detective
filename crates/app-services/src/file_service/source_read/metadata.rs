use domain::{EntryType, FileEncryptionStatus, FileEntry};
use persistence_sqlite::repositories::file_repo::FileRepo;
use std::path::Path;

use super::SourceReadFileHint;
use crate::file_service::{viewer::validate_file_encryption_status, FileServiceError};

pub(super) fn validate_hint_encryption(
    source_conn: &rusqlite::Connection,
    hint: &SourceReadFileHint,
) -> Result<(), FileServiceError> {
    let persisted_status = FileRepo::new(source_conn).find_encryption_status(&hint.file_id)?;
    let status = persisted_status.unwrap_or_else(|| FileEncryptionStatus::from(hint.encrypted));
    validate_file_encryption_status(status)
}

pub(super) fn hint_file_entry(hint: &SourceReadFileHint) -> FileEntry {
    let name = Path::new(&hint.path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&hint.path)
        .to_string();
    FileEntry {
        id: hint.file_id.clone(),
        parent_id: None,
        data_source_id: hint.data_source_id.clone(),
        path: hint.path.clone(),
        name,
        entry_type: EntryType::File,
        size: Some(hint.size),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: hint.encrypted,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}
