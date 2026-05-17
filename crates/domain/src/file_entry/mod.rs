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
