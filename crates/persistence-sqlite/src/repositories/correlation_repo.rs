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
#[path = "../../tests/unit/repositories/correlation_repo.rs"]
mod tests;
