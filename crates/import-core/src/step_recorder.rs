//! Step recorder: compute case state hash and record investigation steps.
//!
//! The case state hash captures key counts (files, artifacts, timeline
//! events, graph nodes/edges) so that replays can detect state drift. Case-wide
//! counts are aggregated through the shared ready-source router — per-source
//! databases are the source of truth, never the case database's legacy tables.

use persistence_sqlite::repositories::notebook_repo::{InvestigationStep, NotebookRepo};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::Path;
use transport::dto::InvestigationStepDto;

#[derive(Debug, thiserror::Error)]
pub enum StepRecorderError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("{0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for StepRecorderError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}

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
/// persists the step through the notebook repository.
pub fn record_step(
    conn: &Connection,
    case_root: &Path,
    input: CaseStepInput<'_>,
) -> Result<InvestigationStepDto, StepRecorderError> {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let case_state_hash = compute_case_state_hash(conn, case_root, input.case_id);
    let step = InvestigationStep {
        id: uuid::Uuid::new_v4().to_string(),
        case_id: input.case_id.to_string(),
        step_kind: input.step_kind.to_string(),
        params_json: input.params_json.to_string(),
        timestamp,
        duration_ms: Some(i64::from(input.duration_ms)),
        case_state_hash: Some(case_state_hash),
        success: Some(input.success),
        error_code: input.error_code.map(str::to_string),
    };
    NotebookRepo::new(conn).record_step(&step)?;
    Ok(InvestigationStepDto {
        id: step.id,
        case_id: step.case_id,
        step_kind: step.step_kind,
        params_json: step.params_json,
        timestamp: step.timestamp,
        duration_ms: input.duration_ms,
        case_state_hash: step.case_state_hash,
        success: input.success,
        error_code: step.error_code,
    })
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
    for (source_id, source_conn) in
        source_kernel::source_db::open_ready_source_connections_read_only(
            conn,
            case_root,
            &domain::CaseId(case_id.to_string()),
        )
        .map_err(source_kernel::source_db::ReadySourceError::into_db_error)?
    {
        let file_count =
            source_kernel::source_db::ready_source_catalog_count(case_root, &source_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        counts.file_count += i64::try_from(file_count)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        let artifact_count =
            source_kernel::source_db::ready_source_artifact_count(case_root, &source_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        counts.artifact_count += i64::try_from(artifact_count)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        let timeline_count =
            source_kernel::source_db::ready_source_timeline_count(case_root, &source_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        counts.timeline_count += i64::try_from(timeline_count)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
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

fn count_table_rows_for_case(conn: &Connection, sql: &str, case_id: &str) -> i64 {
    conn.query_row(sql, [case_id], |row| row.get(0))
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "../tests/unit/step_recorder.rs"]
mod tests;
