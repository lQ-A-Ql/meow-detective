//! Step replay: replay recorded investigation steps against the current case
//! state and compare results.
//!
//! For MVP, full replay is implemented for search steps only. Other step kinds
//! are recognized but returned as "not yet replayable".

use persistence_sqlite::repositories::notebook_repo::{NotebookRepo, StepFilters};
use rusqlite::Connection;
use std::time::Instant;
use thiserror::Error;
use transport::dto::{
    StepReplayDifferDto, StepReplayFailDto, StepReplayMatchDto, StepReplayResultDto,
};

#[derive(Debug, Error)]
pub enum StepReplayError {
    #[error("Database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("Search error: {0}")]
    Search(#[from] crate::search_service::SearchError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

/// Replay recorded investigation steps between `from_step_id` and `to_step_id`
/// (inclusive). For each step, the function parses `params_json`, calls the
/// corresponding service, and compares results.
///
/// # Replayable step kinds (MVP)
///
/// - `"search"` — parses the `query` field from `params_json` and re-runs the
///   search against the current case index. Requires `index_dir`.
///
/// # Non-replayable step kinds
///
/// All other step kinds return a differ entry with `actual: "not yet
/// replayable"`.
pub fn replay_steps(
    conn: &Connection,
    index_dir: &std::path::Path,
    case_id: &str,
    from_step_id: &str,
    to_step_id: &str,
) -> Result<StepReplayResultDto, StepReplayError> {
    let repo = NotebookRepo::new(conn);
    let all_steps = repo
        .list_steps(case_id, &StepFilters::default())
        .map_err(|e| StepReplayError::Other(format!("list steps for replay: {e}")))?;

    // Find the range in the full list (most-recent-first, will reverse below)
    let from_idx = all_steps.iter().position(|s| s.id == from_step_id);
    let to_idx = all_steps.iter().position(|s| s.id == to_step_id);

    let (start, end) = match (from_idx, to_idx) {
        (Some(f), Some(t)) if f <= t => (f, t),
        _ => {
            return Ok(StepReplayResultDto {
                matched_steps: vec![],
                differed_steps: vec![],
                failed_steps: vec![],
                caveats: vec!["Step range not found in case history".to_string()],
            });
        }
    };

    let range = &all_steps[start..=end];

    let mut matched = Vec::new();
    let mut differed = Vec::new();
    let mut failed = Vec::new();
    let mut caveats = Vec::new();

    for step in range {
        let replay_start = Instant::now();

        match step.step_kind.as_str() {
            "search" => {
                let params: Result<serde_json::Value, _> = serde_json::from_str(&step.params_json);
                match params {
                    Ok(params_val) => {
                        let query = params_val
                            .get("query")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if query.is_empty() {
                            failed.push(StepReplayFailDto {
                                step_id: step.id.clone(),
                                step_kind: step.step_kind.clone(),
                                recorded_duration_ms: step.duration_ms.unwrap_or(0) as u32,
                                error: "Missing 'query' field in params_json".to_string(),
                            });
                            continue;
                        }

                        match super::search_service::search_files_real(index_dir, query, 0, 50) {
                            Ok(result) => {
                                let replay_duration = replay_start.elapsed().as_millis() as u32;
                                let recorded_duration = step.duration_ms.unwrap_or(0) as u32;
                                let detail = format!(
                                    "Replayed search for '{}': {} hits (recorded {} ms, replay {} ms)",
                                    query, result.total, recorded_duration, replay_duration,
                                );
                                matched.push(StepReplayMatchDto {
                                    step_id: step.id.clone(),
                                    step_kind: step.step_kind.clone(),
                                    recorded_duration_ms: recorded_duration,
                                    replay_duration_ms: replay_duration,
                                    detail,
                                });
                            }
                            Err(e) => {
                                failed.push(StepReplayFailDto {
                                    step_id: step.id.clone(),
                                    step_kind: step.step_kind.clone(),
                                    recorded_duration_ms: step.duration_ms.unwrap_or(0) as u32,
                                    error: format!("Search replay failed: {e}"),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        failed.push(StepReplayFailDto {
                            step_id: step.id.clone(),
                            step_kind: step.step_kind.clone(),
                            recorded_duration_ms: step.duration_ms.unwrap_or(0) as u32,
                            error: format!("Failed to parse params_json: {e}"),
                        });
                    }
                }
            }
            other => {
                let replay_duration = replay_start.elapsed().as_millis() as u32;
                differed.push(StepReplayDifferDto {
                    step_id: step.id.clone(),
                    step_kind: step.step_kind.clone(),
                    recorded_duration_ms: step.duration_ms.unwrap_or(0) as u32,
                    replay_duration_ms: replay_duration,
                    expected: format!("replay for step_kind '{other}'"),
                    actual: "not yet replayable".to_string(),
                });
            }
        }
    }

    if matched.is_empty() && differed.is_empty() && failed.is_empty() {
        caveats.push("No steps were processed in the given range".to_string());
    }

    Ok(StepReplayResultDto {
        matched_steps: matched,
        differed_steps: differed,
        failed_steps: failed,
        caveats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{CaseId, CaseMeta};
    use persistence_sqlite::repositories::{case_repo::CaseRepo, notebook_repo::NotebookRepo};
    use persistence_sqlite::runner;

    fn setup(case_id: &str) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        runner::run_all(&conn).unwrap();
        CaseRepo::new(&conn)
            .create(&CaseMeta {
                id: CaseId(case_id.to_string()),
                name: "Step Replay Test".to_string(),
                number: None,
                examiner: None,
                notes: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .unwrap();
        conn
    }

    fn insert_step(conn: &Connection, id: &str, case_id: &str, step_kind: &str, params_json: &str) {
        let repo = NotebookRepo::new(conn);
        repo.record_step(
            &persistence_sqlite::repositories::notebook_repo::InvestigationStep {
                id: id.to_string(),
                case_id: case_id.to_string(),
                step_kind: step_kind.to_string(),
                params_json: params_json.to_string(),
                timestamp: "2026-06-14T12:00:00Z".to_string(),
                duration_ms: Some(100),
                case_state_hash: None,
                success: Some(true),
                error_code: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn replay_search_step_parses_query_and_replays() {
        let conn = setup("case-replay-search");
        let tmp = tempfile::TempDir::new().unwrap();
        let index_dir = tmp.path().join("index");

        // Pre-create and populate search index for the replay to find
        let index = search::SearchIndex::create(&index_dir).unwrap();
        let text = search::ExtractedText {
            file_id: "file-1".to_string(),
            content: "forensic needle in a haystack".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 32,
        };
        index.index_documents(&[text], &[]).unwrap();
        // Close index so replay can open it fresh
        drop(index);

        insert_step(
            &conn,
            "step-search-1",
            "case-replay-search",
            "search",
            r#"{"query":"needle","caseId":"case-replay-search"}"#,
        );
        insert_step(
            &conn,
            "step-search-2",
            "case-replay-search",
            "search",
            r#"{"query":"nonexistent","caseId":"case-replay-search"}"#,
        );

        let result = replay_steps(
            &conn,
            &index_dir,
            "case-replay-search",
            "step-search-1",
            "step-search-2",
        )
        .unwrap();

        assert_eq!(result.matched_steps.len(), 2);
        assert!(result.differed_steps.is_empty());
        assert!(result.failed_steps.is_empty());

        // First step should find the needle
        assert!(result.matched_steps[0].detail.contains("1 hits"));
        // Second should find 0
        assert!(result.matched_steps[1].detail.contains("0 hits"));
    }

    #[test]
    fn non_search_steps_are_not_yet_replayable() {
        let conn = setup("case-replay-other");
        let tmp = tempfile::TempDir::new().unwrap();
        let index_dir = tmp.path().join("index");
        std::fs::create_dir_all(&index_dir).unwrap();

        insert_step(
            &conn,
            "step-import-1",
            "case-replay-other",
            "import",
            r#"{"source":"disk.img"}"#,
        );
        insert_step(
            &conn,
            "step-artifact-1",
            "case-replay-other",
            "artifact_extract",
            r#"{"family":"LNK"}"#,
        );

        let result = replay_steps(
            &conn,
            &index_dir,
            "case-replay-other",
            "step-import-1",
            "step-artifact-1",
        )
        .unwrap();

        assert!(result.matched_steps.is_empty());
        assert_eq!(result.differed_steps.len(), 2);
        assert!(result.failed_steps.is_empty());

        for differ in &result.differed_steps {
            assert_eq!(differ.actual, "not yet replayable");
        }
    }

    #[test]
    fn invalid_range_returns_caveat() {
        let conn = setup("case-replay-invalid");
        let tmp = tempfile::TempDir::new().unwrap();
        let index_dir = tmp.path().join("index");
        std::fs::create_dir_all(&index_dir).unwrap();

        let result = replay_steps(
            &conn,
            &index_dir,
            "case-replay-invalid",
            "nonexistent-1",
            "nonexistent-2",
        )
        .unwrap();

        assert!(result.matched_steps.is_empty());
        assert!(result.differed_steps.is_empty());
        assert!(result.failed_steps.is_empty());
        assert_eq!(result.caveats.len(), 1);
        assert!(result.caveats[0].contains("not found"));
    }
}
