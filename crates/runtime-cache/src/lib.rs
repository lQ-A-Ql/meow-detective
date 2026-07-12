//! Runtime cache for performance optimization.
//!
//! This crate provides a temporary SQLite-based cache for:
//! - File handle management for preview operations
//! - Search result pagination caching
//! - Timeline bucket aggregation caching
//! - Preview chunk caching
//!
//! The cache is designed to be ephemeral - deleting the cache database
//! should only result in performance degradation, not data loss.
//! All cached data can be reconstructed from the primary evidence database.

pub mod connection;
pub mod migrations;
pub mod models;
pub mod repositories;

pub use connection::{open_in_memory, open_or_create, CacheError, Result};
pub use models::{CacheEntry, FileHandleCache};
pub use repositories::{CacheRepo, HandleRepo};

/// Runtime cache manager that provides access to all repositories.
pub struct RuntimeCache {
    conn: rusqlite::Connection,
}

impl RuntimeCache {
    /// Open or create a runtime cache at the given path.
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = connection::open_or_create(path)?;
        Ok(Self { conn })
    }

    /// Create an in-memory runtime cache (for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn = connection::open_in_memory()?;
        Ok(Self { conn })
    }

    /// Get a reference to the cache entry repository.
    pub fn cache(&self) -> CacheRepo<'_> {
        CacheRepo::new(&self.conn)
    }

    /// Get a reference to the file handle repository.
    pub fn handles(&self) -> HandleRepo<'_> {
        HandleRepo::new(&self.conn)
    }

    /// Run cleanup on all repositories.
    ///
    /// Returns the total number of entries cleaned up.
    pub fn cleanup_all(&self) -> Result<u64> {
        let cache_cleaned = self.cache().cleanup_expired()?;
        let handles_cleaned = self.handles().cleanup_expired()?;
        Ok(cache_cleaned + handles_cleaned)
    }

    /// Clear all cache data for a specific case.
    pub fn clear_case(&self, case_id: &str) -> Result<u64> {
        let cache_cleared = self.cache().clear_case(case_id)?;
        let handles_cleared = self.handles().clear_case(case_id)?;
        Ok(cache_cleared + handles_cleared)
    }
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
