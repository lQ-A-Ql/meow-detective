use domain::{EntryType, FileEncryptionStatus, FileEntry};
use std::path::Path;

use super::SourceReadFileHint;
use crate::file_service::{viewer::validate_file_encryption_status, FileServiceError};

pub(super) fn validate_hint_encryption(
    _source_conn: &rusqlite::Connection,
    hint: &SourceReadFileHint,
) -> Result<(), FileServiceError> {
    validate_file_encryption_status(FileEncryptionStatus::from(hint.encrypted))
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
