//! Step recorder: compute case state hash and record investigation steps.
//!
//! Delegates persistence to notebook_service. The case state hash captures
//! key counts (files, artifacts, timeline events, graph nodes/edges) so that
//! replays can detect state drift.

use crate::notebook_service::NotebookError;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use transport::dto::InvestigationStepDto;

/// Compute a SHA-256 hash of the current case state from key counts.
///
/// Aggregates (file_count, artifact_count, timeline_count,
/// graph_node_count, graph_edge_count) into a single digest string.
pub fn compute_case_state_hash(conn: &Connection, case_id: &str) -> String {
    let file_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE entry_type = 'file'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let artifact_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
        .unwrap_or(0);

    let timeline_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))
        .unwrap_or(0);

    let graph_node_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM graph_nodes WHERE case_id = ?1",
            [case_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let graph_edge_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM graph_edges WHERE case_id = ?1",
            [case_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let state_str = format!(
        "files:{file_count}|artifacts:{artifact_count}|timeline:{timeline_count}|graph_nodes:{graph_node_count}|graph_edges:{graph_edge_count}"
    );

    let mut hasher = Sha256::new();
    hasher.update(state_str.as_bytes());
    hex::encode(hasher.finalize())
}

/// Record an investigation step, computing the case state hash automatically.
///
/// Creates a UUID id and current timestamp, computes the state hash, and
/// delegates to `notebook_service::record_step`.
pub fn record_step(
    conn: &Connection,
    case_id: &str,
    step_kind: &str,
    params_json: &str,
    duration_ms: u32,
    success: bool,
    error_code: Option<&str>,
) -> Result<InvestigationStepDto, NotebookError> {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let case_state_hash = compute_case_state_hash(conn, case_id);

    crate::notebook_service::record_step(
        conn,
        case_id,
        step_kind,
        params_json,
        &timestamp,
        duration_ms,
        success,
        error_code,
        Some(&case_state_hash),
    )
}

#[cfg(test)]
#[path = "../tests/unit/step_recorder.rs"]
mod tests;
