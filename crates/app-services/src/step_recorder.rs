//! Step recorder: compute case state hash and record investigation steps.
//!
//! Delegates persistence to notebook_service. The case state hash captures
//! key counts (files, artifacts, timeline events, graph nodes/edges) so that
//! replays can detect state drift.

use crate::notebook_service::NotebookError;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::Path;
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

pub fn compute_case_state_hash_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> String {
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

pub struct CaseStepInput<'a> {
    pub case_id: &'a str,
    pub step_kind: &'a str,
    pub params_json: &'a str,
    pub duration_ms: u32,
    pub success: bool,
    pub error_code: Option<&'a str>,
}

pub fn record_step_for_case(
    conn: &Connection,
    case_root: &Path,
    input: CaseStepInput<'_>,
) -> Result<InvestigationStepDto, NotebookError> {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let case_state_hash = compute_case_state_hash_for_case(conn, case_root, input.case_id);

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
    for (_, source_conn) in crate::source_db::open_ready_source_connections(
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
mod tests {
    use super::*;
    use crate::notebook_service;
    use domain::{CaseId, CaseMeta};
    use persistence_sqlite::repositories::{case_repo::CaseRepo, notebook_repo::StepFilters};
    use persistence_sqlite::runner;

    fn setup(case_id: &str) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        runner::run_all(&conn).unwrap();
        CaseRepo::new(&conn)
            .create(&CaseMeta {
                id: CaseId(case_id.to_string()),
                name: "Step Recorder Test".to_string(),
                number: None,
                examiner: None,
                notes: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .unwrap();
        conn
    }

    #[test]
    fn case_state_hash_is_deterministic() {
        let conn = setup("case-hash");
        let hash1 = compute_case_state_hash(&conn, "case-hash");
        let hash2 = compute_case_state_hash(&conn, "case-hash");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn case_state_hash_changes_when_counts_change() {
        let conn = setup("case-hash-change");
        let hash_before = compute_case_state_hash(&conn, "case-hash-change");

        // Insert data source first (required FK), then file entry to change counts
        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path)
             VALUES ('ds-1', 'case-hash-change', 'test-ds', 'LogicalDirectory', '/tmp')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type, size)
             VALUES ('fe-1', 'ds-1', '/test.txt', 'test.txt', 'file', 100)",
            [],
        )
        .unwrap();

        // Insert an artifact to change counts further
        conn.execute(
            "INSERT INTO artifacts (id, case_id, data_source_id, artifact_type, title)
             VALUES ('art-1', 'case-hash-change', 'ds-1', 'TestFamily', 'TestArtifact')",
            [],
        )
        .unwrap();

        let hash_after = compute_case_state_hash(&conn, "case-hash-change");
        assert_ne!(hash_before, hash_after);
        assert_eq!(hash_before.len(), 64);
        assert_eq!(hash_after.len(), 64);
    }

    #[test]
    fn record_step_returns_dto_with_hash() {
        let conn = setup("case-step");
        let dto = record_step(
            &conn,
            "case-step",
            "search",
            r#"{"query":"test"}"#,
            150,
            true,
            None,
        )
        .unwrap();

        assert_eq!(dto.case_id, "case-step");
        assert_eq!(dto.step_kind, "search");
        assert_eq!(dto.params_json, r#"{"query":"test"}"#);
        assert_eq!(dto.duration_ms, 150);
        assert!(dto.success);
        assert!(dto.error_code.is_none());
        assert!(dto.case_state_hash.is_some());
        assert_eq!(dto.case_state_hash.as_deref().unwrap().len(), 64);
        assert!(!dto.id.is_empty());
        assert!(!dto.timestamp.is_empty());
    }

    #[test]
    fn record_step_failure_captures_error_code() {
        let conn = setup("case-fail");
        let dto = record_step(
            &conn,
            "case-fail",
            "import",
            r#"{"source":"disk.img"}"#,
            3000,
            false,
            Some("E_IMPORT_FAILED"),
        )
        .unwrap();

        assert!(!dto.success);
        assert_eq!(dto.error_code.as_deref(), Some("E_IMPORT_FAILED"));
        assert!(dto.case_state_hash.is_some());
    }

    #[test]
    fn recorded_steps_are_persisted_and_listable() {
        let conn = setup("case-persist");
        record_step(
            &conn,
            "case-persist",
            "search",
            r#"{"query":"needle"}"#,
            42,
            true,
            None,
        )
        .unwrap();
        record_step(
            &conn,
            "case-persist",
            "import",
            r#"{"source":"e01"}"#,
            5000,
            false,
            Some("E_CANCELLED"),
        )
        .unwrap();

        let all =
            notebook_service::list_steps(&conn, "case-persist", &StepFilters::default()).unwrap();
        assert_eq!(all.len(), 2);

        let search_filter = StepFilters {
            step_kind: Some("search".to_string()),
            ..Default::default()
        };
        let filtered = notebook_service::list_steps(&conn, "case-persist", &search_filter).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].step_kind, "search");
        assert_eq!(filtered[0].duration_ms, 42);
    }
}
