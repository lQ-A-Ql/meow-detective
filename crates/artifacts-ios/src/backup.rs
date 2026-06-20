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
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::io::Read;

    fn make_manifest_db(files: &[(&str, &str, &str, i32)]) -> Vec<u8> {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE Files (
                    fileID TEXT,
                    domain TEXT,
                    relativePath TEXT,
                    flags INTEGER,
                    file BLOB
                );",
            )
            .expect("create table");
            for (id, domain, path, flags) in files {
                conn.execute(
                    "INSERT INTO Files VALUES (?1, ?2, ?3, ?4, NULL)",
                    rusqlite::params![id, domain, path, flags],
                )
                .expect("insert");
            }
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        buf
    }

    #[test]
    fn parse_manifest_basic() {
        let db = make_manifest_db(&[
            (
                "abc123",
                "HomeDomain",
                "Library/Preferences/com.apple.plist",
                0,
            ),
            ("def456", "AppDomain-com.example", "Documents/notes.txt", 1),
        ]);
        let files = parse_manifest(&db).expect("parse manifest");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file_id, "abc123");
        assert_eq!(files[0].domain, "HomeDomain");
        assert_eq!(
            files[0].relative_path,
            "Library/Preferences/com.apple.plist"
        );
        assert_eq!(files[0].flags, Some(0));
        assert_eq!(files[1].file_id, "def456");
        assert_eq!(files[1].domain, "AppDomain-com.example");
        assert_eq!(files[1].flags, Some(1));
    }

    #[test]
    fn parse_manifest_empty_db() {
        let db = make_manifest_db(&[]);
        let files = parse_manifest(&db).expect("parse manifest");
        assert!(files.is_empty());
    }

    #[test]
    fn parse_manifest_not_a_db() {
        let result = parse_manifest(b"this is not a sqlite database");
        assert!(result.is_err());
    }

    #[test]
    fn parse_manifest_many_files() {
        let entries: Vec<_> = (0..50)
            .map(|i| {
                (
                    format!("hash{:04x}", i),
                    "HomeDomain".to_string(),
                    format!("path/to/file_{}.txt", i),
                    if i % 4 == 0 { 4 } else { 0 },
                )
            })
            .collect();
        let refs: Vec<_> = entries
            .iter()
            .map(|(a, b, c, d)| (a.as_str(), b.as_str(), c.as_str(), *d))
            .collect();
        let db = make_manifest_db(&refs);
        let files = parse_manifest(&db).expect("parse manifest");
        assert_eq!(files.len(), 50);
        assert_eq!(files[4].flags, Some(4)); // i=4 → directory flag
        assert_eq!(files[5].flags, Some(0)); // i=5 → regular file
    }
}
