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
    pub hidden: bool,
    pub system: bool,
    /// True when the file is encrypted via NTFS Encrypting File System (EFS).
    /// Encrypted files cannot be read without the decryption key.
    #[serde(default)]
    pub encrypted: bool,
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
        self.hidden || self.name.starts_with('.')
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
        self.modified_at.or(self.created_at).or(self.accessed_at)
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
#[path = "../../tests/unit/file_entry.rs"]
mod tests;
