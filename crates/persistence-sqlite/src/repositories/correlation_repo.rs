//! Repository for correlation snapshot caching and correlation edge persistence.
//!
//! Covers:
//! - `correlation_snapshots` — cached serialized CorrelationSnapshotDto with hash tracking.
//! - `correlation_edges_cache` — cached correlation edges for incremental updates.

use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection};

// ── Public API ────────────────────────────────────────────────────────

/// Retrieve a cached correlation snapshot for a case, if one exists.
pub fn get_correlation_snapshot_cache(
    conn: &Connection,
    case_id: &str,
) -> DbResult<Option<CachedSnapshotRow>> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT snapshot_json, artifact_hash, artifact_ids_json
         FROM correlation_snapshots WHERE case_id = ?1",
        params![case_id],
        |row| {
            Ok(CachedSnapshotRow {
                snapshot_json: row.get(0)?,
                artifact_hash: row.get(1)?,
                artifact_ids_json: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(DbError::from)
}
pub fn store_correlation_snapshot_cache(
    conn: &Connection,
    case_id: &str,
    snapshot_json: &str,
    generated_at: &str,
    artifact_hash: &str,
    artifact_ids_json: &str,
) -> DbResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO correlation_snapshots
         (case_id, snapshot_json, generated_at, artifact_hash, artifact_ids_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            case_id,
            snapshot_json,
            generated_at,
            artifact_hash,
            artifact_ids_json
        ],
    )?;
    Ok(())
}

/// Clear correlation cache for a case (both snapshots and edges).
pub fn clear_correlation_cache(conn: &Connection, case_id: &str) -> DbResult<()> {
    conn.execute(
        "DELETE FROM correlation_snapshots WHERE case_id = ?1",
        params![case_id],
    )?;
    conn.execute(
        "DELETE FROM correlation_edges_cache WHERE case_id = ?1",
        params![case_id],
    )?;
    Ok(())
}

/// Compute an artifact hash over (sorted id, created_at) pairs.
/// Returns the hex-encoded SHA-256 digest.
pub fn compute_artifact_hash_hex(conn: &Connection) -> DbResult<String> {
    use sha2::{Digest, Sha256};
    let mut stmt = conn.prepare("SELECT id, created_at FROM artifacts ORDER BY id")?;
    let mut hasher = Sha256::new();
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let created_at: String = row.get(1)?;
        Ok((id, created_at))
    })?;
    for row in rows {
        let (id, created_at) = row?;
        hasher.update(id.as_bytes());
        hasher.update(b"|");
        hasher.update(created_at.as_bytes());
        hasher.update(b"\n");
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Resolve the case_id from the artifacts table (returns the first distinct id).
pub fn resolve_case_id(conn: &Connection) -> DbResult<Option<String>> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT DISTINCT case_id FROM artifacts LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(DbError::from)
}

/// Collect all artifact IDs ordered by id.
pub fn collect_artifact_ids(conn: &Connection) -> DbResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT id FROM artifacts ORDER BY id")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row?);
    }
    Ok(ids)
}

// ── Types ─────────────────────────────────────────────────────────────

/// Deserialized row from the correlation_snapshots table.
#[derive(Debug, Clone)]
pub struct CachedSnapshotRow {
    pub snapshot_json: String,
    pub artifact_hash: String,
    pub artifact_ids_json: String,
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::open_in_memory;

    fn setup_db() -> Connection {
        let conn = open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE correlation_snapshots (
                case_id TEXT PRIMARY KEY NOT NULL,
                snapshot_json TEXT NOT NULL,
                generated_at TEXT NOT NULL,
                artifact_hash TEXT NOT NULL,
                artifact_ids_json TEXT NOT NULL
            );
            CREATE TABLE correlation_edges_cache (
                case_id TEXT NOT NULL,
                edge_id TEXT NOT NULL,
                edge_data TEXT NOT NULL,
                PRIMARY KEY (case_id, edge_id)
            );
            CREATE TABLE artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                attrs TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn store_and_get_snapshot_cache() {
        let conn = setup_db();

        let row = get_correlation_snapshot_cache(&conn, "case-1").unwrap();
        assert!(row.is_none());

        store_correlation_snapshot_cache(
            &conn,
            "case-1",
            r#"{"nodes":[]}"#,
            "2026-01-01T00:00:00Z",
            "abc123",
            r#"["a1"]"#,
        )
        .unwrap();

        let row = get_correlation_snapshot_cache(&conn, "case-1")
            .unwrap()
            .expect("should exist");
        assert_eq!(row.snapshot_json, r#"{"nodes":[]}"#);
        assert_eq!(row.artifact_hash, "abc123");
        assert_eq!(row.artifact_ids_json, r#"["a1"]"#);
    }

    #[test]
    fn store_cache_overwrites_previous() {
        let conn = setup_db();

        store_correlation_snapshot_cache(
            &conn,
            "case-1",
            "old",
            "2026-01-01T00:00:00Z",
            "hash1",
            "[]",
        )
        .unwrap();
        store_correlation_snapshot_cache(
            &conn,
            "case-1",
            "new",
            "2026-01-02T00:00:00Z",
            "hash2",
            r#"["a2"]"#,
        )
        .unwrap();

        let row = get_correlation_snapshot_cache(&conn, "case-1")
            .unwrap()
            .unwrap();
        assert_eq!(row.snapshot_json, "new");
        assert_eq!(row.artifact_hash, "hash2");
    }

    #[test]
    fn clear_cache_removes_both_tables() {
        let conn = setup_db();

        store_correlation_snapshot_cache(
            &conn,
            "case-1",
            "test",
            "2026-01-01T00:00:00Z",
            "hash",
            "[]",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO correlation_edges_cache (case_id, edge_id, edge_data) VALUES ('case-1', 'e1', '{}')",
            [],
        )
        .unwrap();

        clear_correlation_cache(&conn, "case-1").unwrap();

        assert!(get_correlation_snapshot_cache(&conn, "case-1")
            .unwrap()
            .is_none());

        let edge_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM correlation_edges_cache WHERE case_id = 'case-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(edge_count, 0);
    }

    #[test]
    fn compute_artifact_hash_is_stable() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title, created_at)
             VALUES ('a1', 'case-1', 'Artifact 1', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title, created_at)
             VALUES ('a2', 'case-1', 'Artifact 2', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let h1 = compute_artifact_hash_hex(&conn).unwrap();
        let h2 = compute_artifact_hash_hex(&conn).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex is 64 chars
    }

    #[test]
    fn compute_artifact_hash_changes_when_data_changes() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title, created_at)
             VALUES ('a1', 'case-1', 'Artifact 1', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let h1 = compute_artifact_hash_hex(&conn).unwrap();

        conn.execute(
            "INSERT INTO artifacts (id, case_id, title, created_at)
             VALUES ('a2', 'case-1', 'Artifact 2', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let h2 = compute_artifact_hash_hex(&conn).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn resolve_case_id_returns_first_distinct() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title) VALUES ('a1', 'case-1', 'A1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title) VALUES ('a2', 'case-1', 'A2')",
            [],
        )
        .unwrap();

        let case_id = resolve_case_id(&conn).unwrap();
        assert_eq!(case_id.as_deref(), Some("case-1"));
    }

    #[test]
    fn resolve_case_id_returns_none_when_empty() {
        let conn = setup_db();
        let case_id = resolve_case_id(&conn).unwrap();
        assert!(case_id.is_none());
    }

    #[test]
    fn collect_artifact_ids_returns_sorted() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title) VALUES ('a2', 'case-1', 'A2')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO artifacts (id, case_id, title) VALUES ('a1', 'case-1', 'A1')",
            [],
        )
        .unwrap();

        let ids = collect_artifact_ids(&conn).unwrap();
        assert_eq!(ids, vec!["a1", "a2"]);
    }
}
