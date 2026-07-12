use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single browser history visit (navigation record).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserVisit {
    pub url: String,
    pub title: Option<String>,
    pub visit_time: Option<DateTime<Utc>>,
    pub visit_count: i64,
    pub browser: String,
    pub profile: Option<String>,
}

/// A browser download record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserDownload {
    pub url: String,
    pub target_path: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub total_bytes: i64,
    pub browser: String,
    pub profile: Option<String>,
}

/// A browser cookie record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCookie {
    pub domain: String,
    pub name: String,
    /// Truncated preview of the cookie value (first 128 bytes).
    /// Will be `None` for encrypted values that appear to be ciphertext.
    pub value_preview: Option<String>,
    pub expiry: Option<DateTime<Utc>>,
    pub secure: bool,
    pub http_only: bool,
    /// Raw `same_site` column value: -1 = unspecified, 0 = none, 1 = lax, 2 = strict.
    pub same_site: Option<i64>,
}

/// A single tab entry from a restored browser session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSessionTab {
    pub url: String,
    pub title: Option<String>,
    pub window_index: i32,
    pub tab_index: i32,
    pub last_active: Option<DateTime<Utc>>,
}

/// A saved browser password entry.
///
/// Chromium stores the actual password encrypted with DPAPI. This parser
/// extracts only metadata and never attempts decryption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserPassword {
    pub url: String,
    pub username: String,
    pub password_preview: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub times_used: i64,
    pub browser: String,
    pub profile: Option<String>,
}
