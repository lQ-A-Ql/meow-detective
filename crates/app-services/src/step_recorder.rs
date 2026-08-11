//! Step recorder: compute case state hash and record investigation steps.
//!
//! Delegates persistence to notebook_service. The case state hash captures
//! key counts (files, artifacts, timeline events, graph nodes/edges) so that
//! replays can detect state drift. Case-wide counts are aggregated through
//! the shared ready-source router — per-source databases are the source of
//! truth, never the case database's legacy tables.

use crate::notebook_service::NotebookError;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::Path;
use transport::dto::InvestigationStepDto;

/// Compute a SHA-256 hash of the current case state from key counts.
///
/// Aggregates (file_count, artifact_count, timeline_count,
/// graph_node_count, graph_edge_count) across every ready source database
/// into a single digest string. Sources that are not ready contribute
/// nothing; a source that cannot be opened fails the whole aggregation back
/// to an all-zero digest rather than a partial one.
pub fn compute_case_state_hash(conn: &Connection, case_root: &Path, case_id: &str) -> String {
    let counts = aggregate_source_counts(conn, case_root, case_id).unwrap_or_default();
    let state_str = format!(
        "files:{}|artifacts:{}|timeline:{}|graph_nodes:{}|graph_edges:{}",
        counts.file_count,
        counts.artifact_count,
        counts.timeline_count,
        counts.graph_node_count,
        counts.graph_edge_count
    );

    let mut hasher = Sha256::new();
    hasher.update(state_str.as_bytes());
    hex::encode(hasher.finalize())
}

/// Parameters of one recorded investigation step.
pub struct CaseStepInput<'a> {
    pub case_id: &'a str,
    pub step_kind: &'a str,
    pub params_json: &'a str,
    pub duration_ms: u32,
    pub success: bool,
    pub error_code: Option<&'a str>,
}

/// Record an investigation step, computing the case state hash automatically.
///
/// Creates a UUID id and current timestamp, computes the state hash, and
/// delegates to `notebook_service::record_step`.
pub fn record_step(
    conn: &Connection,
    case_root: &Path,
    input: CaseStepInput<'_>,
) -> Result<InvestigationStepDto, NotebookError> {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let case_state_hash = compute_case_state_hash(conn, case_root, input.case_id);

    crate::notebook_service::record_step(
        conn,
        input.case_id,
        input.step_kind,
        input.params_json,
        &timestamp,
        input.duration_ms,
        input.success,
        input.error_code,
        Some(&case_state_hash),
    )
}

#[derive(Default)]
struct SourceCounts {
    file_count: i64,
    artifact_count: i64,
    timeline_count: i64,
    graph_node_count: i64,
    graph_edge_count: i64,
}

fn aggregate_source_counts(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<SourceCounts, persistence_sqlite::DbError> {
    let mut counts = SourceCounts::default();
    for (_, source_conn) in crate::source_db::open_ready_source_connections_read_only(
        conn,
        case_root,
        &domain::CaseId(case_id.to_string()),
    )
    .map_err(crate::source_db::ReadySourceError::into_db_error)?
    {
        counts.file_count += count_table_rows_no_params(
            &source_conn,
            "SELECT COUNT(*) FROM file_entries WHERE entry_type = 'file'",
        );
        counts.artifact_count +=
            count_table_rows_no_params(&source_conn, "SELECT COUNT(*) FROM artifacts");
        counts.timeline_count +=
            count_table_rows_no_params(&source_conn, "SELECT COUNT(*) FROM timeline_events");
        counts.graph_node_count += count_table_rows_for_case(
            &source_conn,
            "SELECT COUNT(*) FROM graph_nodes WHERE case_id = ?1",
            case_id,
        );
        counts.graph_edge_count += count_table_rows_for_case(
            &source_conn,
            "SELECT COUNT(*) FROM graph_edges WHERE case_id = ?1",
            case_id,
        );
    }
    Ok(counts)
}

fn count_table_rows_no_params(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0)).unwrap_or(0)
}

fn count_table_rows_for_case(conn: &Connection, sql: &str, case_id: &str) -> i64 {
    conn.query_row(sql, [case_id], |row| row.get(0))
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "../tests/unit/step_recorder.rs"]
mod tests;
