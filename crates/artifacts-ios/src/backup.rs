//! Parse iOS backup Manifest.db (SQLite), listing files contained in the backup.
//!
//! An iTunes/Finder backup stores a Manifest.db at its root. The `Files` table
//! maps backup hashed file names to their original relative paths, domains, and
//! file IDs.

use crate::{open_sqlite_from_bytes, IosArtifactError};
use serde::{Deserialize, Serialize};

/// A file entry from an iOS backup Manifest.db.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosBackupFile {
    /// Hex-encoded SHA1 hash used as the backup file name (e.g. "abcdef1234...").
    pub file_id: String,
    /// The backup domain (e.g. "HomeDomain", "AppDomain-com.example").
    pub domain: String,
    /// The original relative path inside the domain.
    pub relative_path: String,
    /// File flags (0 = regular file, 4 = directory, etc.).
    pub flags: Option<i32>,
}

/// Parse an iOS backup `Manifest.db` and return the list of files.
///
/// Reads the `Files` table which contains `fileID`, `domain`, `relativePath`,
/// and `flags` columns.
pub fn parse_manifest(data: &[u8]) -> Result<Vec<IosBackupFile>, IosArtifactError> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    let mut stmt = conn.prepare("SELECT fileID, domain, relativePath, flags FROM Files")?;

    let rows = stmt.query_map([], |row| {
        Ok(IosBackupFile {
            file_id: row.get(0)?,
            domain: row.get(1)?,
            relative_path: row.get(2)?,
            flags: crate::row_get_opt(row, "flags"),
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(file) => results.push(file),
            Err(e) => {
                tracing::warn!("skipping Manifest.db row: {}", e);
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
#[path = "../tests/unit/backup.rs"]
mod tests;
