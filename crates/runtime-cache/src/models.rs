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
    pub const PREVIEW_DESCRIPTORS: &str = "preview_descriptors";
}
