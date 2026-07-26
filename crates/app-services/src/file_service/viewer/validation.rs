use domain::{EntryType, FileEncryptionStatus, FileEntry};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;

use crate::file_service::FileServiceError;

pub(crate) fn validate_readable_file_entry(
    conn: &Connection,
    entry: &FileEntry,
) -> Result<(), FileServiceError> {
    if entry.entry_type != EntryType::File {
        return Err(FileServiceError::invalid_input(
            "Cannot read a directory as a file",
        ));
    }
    let status = FileRepo::new(conn)
        .find_encryption_status(&entry.id)?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;
    validate_file_encryption_status(status)
}

pub(crate) fn validate_file_encryption_status(
    status: FileEncryptionStatus,
) -> Result<(), FileServiceError> {
    match status {
        FileEncryptionStatus::Clear => Ok(()),
        FileEncryptionStatus::Encrypted => Err(FileServiceError::Unsupported(
            "NTFS EFS-encrypted content is unavailable without a decryption key".to_string(),
        )),
        FileEncryptionStatus::Unknown => Err(FileServiceError::Unsupported(
            "File encryption status is unknown; content access is blocked until the data source is re-enumerated"
                .to_string(),
        )),
    }
}
