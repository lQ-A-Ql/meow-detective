//! Export file content from evidence to a host filesystem destination.
//!
//! Handles streaming the evidence reader through a temp file and
//! atomically renaming on success, with automatic cleanup on error.

use std::io::Write;
use std::path::Path;

use domain::FileEntryId;
use rusqlite::Connection;

use crate::file_service::{self, FileServiceError};

/// Extract a file from evidence to a destination path on the host filesystem.
///
/// Writes to a temp file first. On success the temp file is atomically
/// renamed to `destination_path`. The temp file is cleaned up on any error.
pub fn extract_file_to_destination(
    conn: &Connection,
    file_id: &str,
    destination_path: &Path,
    overwrite: bool,
) -> Result<u64, FileServiceError> {
    let mut reader =
        file_service::open_file_content_by_id(conn, &FileEntryId(file_id.to_string()))?;

    // --- destination validation ---
    if destination_path.exists() && destination_path.is_dir() {
        return Err(FileServiceError::invalid_input(
            "destinationPath must point to a file, not a directory",
        ));
    }
    if destination_path.exists() && !overwrite {
        return Err(FileServiceError::invalid_input(
            "destinationPath already exists; set overwrite=true to replace it",
        ));
    }
    if let Some(parent) = destination_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // --- unique temp path ---
    let temp_path = destination_path.with_extension(format!(
        "{}{}.tmp",
        destination_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!("{ext}."))
            .unwrap_or_default(),
        uuid::Uuid::new_v4()
    ));

    // --- extract with cleanup on error ---
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;

    let bytes = std::io::copy(&mut reader, &mut output).map_err(|err| {
        let _ = std::fs::remove_file(&temp_path);
        FileServiceError::Io(err)
    })?;

    output.flush().map_err(|err| {
        let _ = std::fs::remove_file(&temp_path);
        FileServiceError::Io(err)
    })?;
    output.sync_all().map_err(|err| {
        let _ = std::fs::remove_file(&temp_path);
        FileServiceError::Io(err)
    })?;
    drop(output);

    if overwrite && destination_path.exists() {
        std::fs::remove_file(destination_path).map_err(|err| {
            let _ = std::fs::remove_file(&temp_path);
            FileServiceError::Io(err)
        })?;
    }
    std::fs::rename(&temp_path, destination_path).map_err(|err| {
        let _ = std::fs::remove_file(&temp_path);
        FileServiceError::Io(err)
    })?;

    Ok(bytes)
}
