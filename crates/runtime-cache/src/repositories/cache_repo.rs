//! Repository for generic cache entries.

use crate::connection::{CacheError, Result};
use crate::models::CacheEntry;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};

/// Repository for managing generic cache entries.
pub struct CacheRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CacheRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Get a cache entry by key.
    ///
    /// Returns None if not found or expired.
    /// Updates last_accessed_at on successful retrieval.
    pub fn get(&self, key: &str) -> Result<Option<CacheEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT cache_key, namespace, case_id, value_json, created_at, expires_at, last_accessed_at
             FROM cache_entries
             WHERE cache_key = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
        )?;

        let now = Utc::now().to_rfc3339();
        let result = stmt.query_row(params![key, now], |row| {
            let value_str: String = row.get(3)?;
            Ok(CacheEntry {
                cache_key: row.get(0)?,
                namespace: row.get(1)?,
                case_id: row.get(2)?,
                value_json: serde_json::from_str(&value_str).unwrap_or_default(),
                created_at: parse_datetime(&row.get::<_, String>(4)?),
                expires_at: row.get::<_, Option<String>>(5)?.map(|s| parse_datetime(&s)),
                last_accessed_at: parse_datetime(&row.get::<_, String>(6)?),
            })
        });

        match result {
            Ok(entry) => {
                // Update last_accessed_at
                self.conn.execute(
                    "UPDATE cache_entries SET last_accessed_at = ?1 WHERE cache_key = ?2",
                    params![now, key],
                )?;
                Ok(Some(entry))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CacheError::Sqlite(e)),
        }
    }

    /// Set a cache entry.
    ///
    /// Inserts or replaces the entry.
    pub fn set(&self, entry: &CacheEntry) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO cache_entries (cache_key, namespace, case_id, value_json, created_at, expires_at, last_accessed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.cache_key,
                entry.namespace,
                entry.case_id,
                entry.value_json.to_string(),
                entry.created_at.to_rfc3339(),
                entry.expires_at.map(|dt| dt.to_rfc3339()),
                entry.last_accessed_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Delete a cache entry by key.
    pub fn delete(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM cache_entries WHERE cache_key = ?1", params![key])?;
        Ok(())
    }

    /// Get a cache entry or insert one using the factory function.
    ///
    /// If the entry exists and is not expired, returns it.
    /// Otherwise, calls the factory, stores the result, and returns it.
    pub fn get_or_insert<F>(&self, key: &str, namespace: &str, ttl: Duration, factory: F) -> Result<CacheEntry>
    where
        F: FnOnce() -> Result<serde_json::Value>,
    {
        if let Some(entry) = self.get(key)? {
            return Ok(entry);
        }

        let now = Utc::now();
        let value = factory()?;
        let entry = CacheEntry {
            cache_key: key.to_string(),
            namespace: namespace.to_string(),
            case_id: None,
            value_json: value,
            created_at: now,
            expires_at: Some(now + ttl),
            last_accessed_at: now,
        };
        self.set(&entry)?;
        Ok(entry)
    }

    /// Delete all expired entries.
    ///
    /// Returns the number of deleted entries.
    pub fn cleanup_expired(&self) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let count = self.conn.execute(
            "DELETE FROM cache_entries WHERE expires_at IS NOT NULL AND expires_at <= ?1",
            params![now],
        )?;
        Ok(count as u64)
    }

    /// Delete all entries in a namespace.
    ///
    /// Returns the number of deleted entries.
    pub fn clear_namespace(&self, namespace: &str) -> Result<u64> {
        let count = self.conn.execute(
            "DELETE FROM cache_entries WHERE namespace = ?1",
            params![namespace],
        )?;
        Ok(count as u64)
    }

    /// Delete all entries for a case.
    ///
    /// Returns the number of deleted entries.
    pub fn clear_case(&self, case_id: &str) -> Result<u64> {
        let count = self.conn.execute(
            "DELETE FROM cache_entries WHERE case_id = ?1",
            params![case_id],
        )?;
        Ok(count as u64)
    }

    /// Count entries in a namespace.
    pub fn count_namespace(&self, namespace: &str) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM cache_entries WHERE namespace = ?1",
            params![namespace],
            |r| r.get(0),
        )?;
        Ok(count as u64)
    }
}

fn parse_datetime(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_set_get_delete() {
        let conn = crate::connection::open_in_memory().unwrap();
        let repo = CacheRepo::new(&conn);

        let entry = CacheEntry {
            cache_key: "test-key".to_string(),
            namespace: "test".to_string(),
            case_id: None,
            value_json: serde_json::json!({"data": "value"}),
            created_at: Utc::now(),
            expires_at: None,
            last_accessed_at: Utc::now(),
        };

        repo.set(&entry).unwrap();

        let loaded = repo.get("test-key").unwrap().unwrap();
        assert_eq!(loaded.cache_key, "test-key");
        assert_eq!(loaded.value_json, serde_json::json!({"data": "value"}));

        repo.delete("test-key").unwrap();
        assert!(repo.get("test-key").unwrap().is_none());
    }

    #[test]
    fn cache_get_or_insert() {
        let conn = crate::connection::open_in_memory().unwrap();
        let repo = CacheRepo::new(&conn);

        let entry = repo
            .get_or_insert("key1", "ns", Duration::seconds(60), || {
                Ok(serde_json::json!({"computed": 42}))
            })
            .unwrap();

        assert_eq!(entry.value_json, serde_json::json!({"computed": 42}));

        // Second call should return cached
        let entry2 = repo.get_or_insert("key1", "ns", Duration::seconds(60), || {
            panic!("Should not be called");
        })
        .unwrap();

        assert_eq!(entry2.value_json, serde_json::json!({"computed": 42}));
    }

    #[test]
    fn cache_cleanup_expired() {
        let conn = crate::connection::open_in_memory().unwrap();
        let repo = CacheRepo::new(&conn);

        let past = Utc::now() - Duration::seconds(60);
        let entry = CacheEntry {
            cache_key: "expired".to_string(),
            namespace: "test".to_string(),
            case_id: None,
            value_json: serde_json::json!(null),
            created_at: past,
            expires_at: Some(past + Duration::seconds(30)),
            last_accessed_at: past,
        };
        repo.set(&entry).unwrap();

        assert!(repo.get("expired").unwrap().is_none());

        let cleaned = repo.cleanup_expired().unwrap();
        assert_eq!(cleaned, 1);
    }
}
