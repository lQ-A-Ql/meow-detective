//! Repository for correlation snapshot caching and correlation edge persistence.
//!
//! Covers:
//! - `correlation_snapshots` — cached serialized CorrelationSnapshotDto with hash tracking.
//! - `correlation_edges_cache` — cached correlation edges for incremental updates.

use persistence_sqlite::connection::{DbError, DbResult};
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

/// Resolve the case_id from the artifacts table (returns the first distinct id).
pub fn resolve_case_id(conn: &Connection) -> DbResult<Option<String>> {
    if let Some(source_id) = crate::detail::source_id(conn)? {
        let artifact_count = super::artifact_repo::ArtifactRepo::new(conn).count()?;
        if artifact_count == 0 {
            return Ok(None);
        }
        let mut statement =
            conn.prepare("SELECT case_id FROM data_sources WHERE id = ?1 ORDER BY id LIMIT 2")?;
        let cases = statement
            .query_map([source_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        return match cases.as_slice() {
            [case_id] => Ok(Some(case_id.clone())),
            [] => Ok(None),
            _ => Err(DbError::System(
                "artifact source catalog spans multiple cases".into(),
            )),
        };
    }
    Err(DbError::System("source identity is missing".into()))
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
