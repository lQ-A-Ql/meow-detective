use crate::DataSourceId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FileEntryId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntryType {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub id: FileEntryId,
    pub parent_id: Option<FileEntryId>,
    pub data_source_id: DataSourceId,
    pub path: String,
    pub name: String,
    pub entry_type: EntryType,
    pub size: Option<u64>,
    pub ext: Option<String>,
    pub deleted: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub accessed_at: Option<DateTime<Utc>>,
    pub changed_at: Option<DateTime<Utc>>,
    pub hash_sha256: Option<String>,
}

/// Domain behavior for FileEntry
impl FileEntry {
    /// Check if this entry is a file (not a directory).
    pub fn is_file(&self) -> bool {
        self.entry_type == EntryType::File
    }

    /// Check if this entry is a directory.
    pub fn is_directory(&self) -> bool {
        self.entry_type == EntryType::Directory
    }

    /// Check if this entry is hidden (name starts with '.').
    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('.')
    }

    /// Get the file extension (without the dot).
    ///
    /// Returns `None` if the file has no extension or if the name
    /// has no dot (or the dot is the first character, like ".gitignore").
    pub fn extension(&self) -> Option<&str> {
        // Hidden files starting with '.' have no extension
        if self.name.starts_with('.') {
            return None;
        }
        self.name
            .rsplit('.')
            .next()
            .filter(|e| *e != self.name && !e.is_empty())
    }

    /// Get the file size, defaulting to 0 for directories.
    pub fn size_or_zero(&self) -> u64 {
        self.size.unwrap_or(0)
    }

    /// Get the most relevant timestamp (modified > created > accessed).
    pub fn best_timestamp(&self) -> Option<DateTime<Utc>> {
        self.modified_at
            .or(self.created_at)
            .or(self.accessed_at)
    }

    /// Check if this file has been deleted (marked as deleted in filesystem).
    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    /// Check if this file has a SHA-256 hash computed.
    pub fn has_hash(&self) -> bool {
        self.hash_sha256.as_ref().is_some_and(|h| !h.is_empty())
    }

    /// Get the parent ID, or None if this is a root entry.
    pub fn parent_id(&self) -> Option<&FileEntryId> {
        self.parent_id.as_ref()
    }

    /// Check if this is a root entry (no parent).
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file(name: &str) -> FileEntry {
        FileEntry {
            id: FileEntryId("test".to_string()),
            parent_id: Some(FileEntryId("parent".to_string())),
            data_source_id: crate::DataSourceId("ds".to_string()),
            path: format!("/test/{}", name),
            name: name.to_string(),
            entry_type: EntryType::File,
            size: Some(1024),
            ext: None,
            deleted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }
    }

    fn make_dir(name: &str) -> FileEntry {
        let mut entry = make_file(name);
        entry.entry_type = EntryType::Directory;
        entry.size = None;
        entry
    }

    #[test]
    fn is_file_true() {
        assert!(make_file("test.txt").is_file());
    }

    #[test]
    fn is_file_false() {
        assert!(!make_dir("docs").is_file());
    }

    #[test]
    fn is_directory_true() {
        assert!(make_dir("docs").is_directory());
    }

    #[test]
    fn is_hidden_true() {
        assert!(make_file(".gitignore").is_hidden());
    }

    #[test]
    fn is_hidden_false() {
        assert!(!make_file("readme.txt").is_hidden());
    }

    #[test]
    fn extension_basic() {
        assert_eq!(make_file("test.txt").extension(), Some("txt"));
        assert_eq!(make_file("archive.tar.gz").extension(), Some("gz"));
    }

    #[test]
    fn extension_none() {
        assert_eq!(make_file("Makefile").extension(), None);
        // .gitignore has no extension (dot is first char, so rsplit returns "gitignore")
        // but our implementation filters out cases where the dot is the first char
        // because rsplit('.').next() on ".gitignore" returns "gitignore" which equals the stem
        assert_eq!(make_file(".gitignore").extension(), None);
    }

    #[test]
    fn size_or_zero_file() {
        assert_eq!(make_file("test.txt").size_or_zero(), 1024);
    }

    #[test]
    fn size_or_zero_directory() {
        assert_eq!(make_dir("docs").size_or_zero(), 0);
    }

    #[test]
    fn is_root_true() {
        let mut entry = make_file("root.txt");
        entry.parent_id = None;
        assert!(entry.is_root());
    }

    #[test]
    fn is_root_false() {
        assert!(!make_file("child.txt").is_root());
    }

    #[test]
    fn is_deleted() {
        let mut entry = make_file("deleted.txt");
        entry.deleted = true;
        assert!(entry.is_deleted());
    }

    #[test]
    fn has_hash_true() {
        let mut entry = make_file("test.txt");
        entry.hash_sha256 = Some("abc123".to_string());
        assert!(entry.has_hash());
    }

    #[test]
    fn has_hash_false() {
        assert!(!make_file("test.txt").has_hash());
    }
}
