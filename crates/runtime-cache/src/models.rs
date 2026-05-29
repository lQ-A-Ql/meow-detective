//! Data models for the runtime cache.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A cache entry with optional TTL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub cache_key: String,
    pub namespace: String,
    pub case_id: Option<String>,
    pub value_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_accessed_at: DateTime<Utc>,
}

/// A cached file handle for preview operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHandleCache {
    pub handle_id: String,
    pub case_id: String,
    pub object_id: String,
    pub opened_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub access_mode: String,
}

/// Cache namespaces for organizing different types of cached data.
pub mod namespaces {
    pub const FILE_HANDLES: &str = "file_handles";
    pub const SEARCH_RESULTS: &str = "search_results";
    pub const TIMELINE_BUCKETS: &str = "timeline_buckets";
    pub const PREVIEW_CHUNKS: &str = "preview_chunks";
}
