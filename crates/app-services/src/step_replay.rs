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
#[path = "../tests/unit/step_replay.rs"]
mod tests;
