use super::super::CorrelationError;
use persistence_sqlite::repositories::correlation_repo;
use rusqlite::Connection;
use std::collections::BTreeSet;
use transport::dto::CorrelationSnapshotDto;

pub(super) struct CachedSnapshot {
    pub snapshot_json: String,
    pub artifact_hash: String,
    pub artifact_ids_json: String,
}

pub(super) fn get_cached_snapshot(
    conn: &Connection,
    case_id: &str,
) -> Result<Option<CachedSnapshot>, CorrelationError> {
    let cached = correlation_repo::get_correlation_snapshot_cache(conn, case_id)
        .map_err(|error| CorrelationError::Other(error.to_string()))?;
    Ok(cached.map(|item| CachedSnapshot {
        snapshot_json: item.snapshot_json,
        artifact_hash: item.artifact_hash,
        artifact_ids_json: item.artifact_ids_json,
    }))
}

pub(super) fn store_cached_snapshot(
    conn: &Connection,
    case_id: &str,
    snapshot: &CorrelationSnapshotDto,
    artifact_hash: &str,
    artifact_ids_json: &str,
) -> Result<(), CorrelationError> {
    let json = serde_json::to_string(snapshot).map_err(|error| {
        CorrelationError::Other(format!("serialize snapshot for cache: {error}"))
    })?;
    correlation_repo::store_correlation_snapshot_cache(
        conn,
        case_id,
        &json,
        &snapshot.generated_at,
        artifact_hash,
        artifact_ids_json,
    )
    .map_err(|error| CorrelationError::Other(format!("store cached snapshot: {error}")))?;
    Ok(())
}

pub fn invalidate_correlation_cache(
    conn: &Connection,
    case_id: &str,
) -> Result<(), CorrelationError> {
    correlation_repo::clear_correlation_cache(conn, case_id).map_err(|error| {
        CorrelationError::Other(format!("invalidate correlation cache: {error}"))
    })?;
    Ok(())
}

pub(super) fn compute_artifact_hash(conn: &Connection) -> Result<String, CorrelationError> {
    correlation_repo::compute_artifact_hash_hex(conn)
        .map_err(|error| CorrelationError::Other(format!("compute artifact hash: {error}")))
}

pub(super) fn collect_artifact_ids(
    conn: &Connection,
) -> Result<BTreeSet<String>, CorrelationError> {
    let ids = correlation_repo::collect_artifact_ids(conn)
        .map_err(|error| CorrelationError::Other(format!("collect artifact ids: {error}")))?;
    Ok(ids.into_iter().collect())
}

pub(super) fn resolve_case_id(conn: &Connection) -> Result<Option<String>, CorrelationError> {
    correlation_repo::resolve_case_id(conn)
        .map_err(|error| CorrelationError::Other(format!("resolve case_id: {error}")))
}
