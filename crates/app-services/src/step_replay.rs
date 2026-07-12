//! Step replay: replay recorded investigation steps against the current case
//! state and compare results.
//!
//! For MVP, full replay is implemented for search steps only. Other step kinds
//! are recognized but returned as "not yet replayable".

use persistence_sqlite::repositories::notebook_repo::{
    InvestigationStep, NotebookRepo, StepFilters,
};
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
    let all_steps = NotebookRepo::new(conn)
        .list_steps(case_id, &StepFilters::default())
        .map_err(|e| StepReplayError::Other(format!("list steps for replay: {e}")))?;
    let Some(range) = replay_range(&all_steps, from_step_id, to_step_id) else {
        return Ok(missing_range_result());
    };

    let mut replay = ReplayAccumulator::default();
    for step in range {
        replay_step(step, index_dir, &mut replay);
    }
    Ok(replay.finish())
}

fn replay_range<'a>(
    steps: &'a [InvestigationStep],
    from_step_id: &str,
    to_step_id: &str,
) -> Option<&'a [InvestigationStep]> {
    let from_idx = steps.iter().position(|step| step.id == from_step_id)?;
    let to_idx = steps.iter().position(|step| step.id == to_step_id)?;
    (from_idx <= to_idx).then_some(&steps[from_idx..=to_idx])
}

fn missing_range_result() -> StepReplayResultDto {
    StepReplayResultDto {
        matched_steps: vec![],
        differed_steps: vec![],
        failed_steps: vec![],
        caveats: vec!["Step range not found in case history".to_string()],
    }
}

#[derive(Default)]
struct ReplayAccumulator {
    matched: Vec<StepReplayMatchDto>,
    differed: Vec<StepReplayDifferDto>,
    failed: Vec<StepReplayFailDto>,
}

impl ReplayAccumulator {
    fn finish(self) -> StepReplayResultDto {
        let caveats =
            if self.matched.is_empty() && self.differed.is_empty() && self.failed.is_empty() {
                vec!["No steps were processed in the given range".to_string()]
            } else {
                Vec::new()
            };
        StepReplayResultDto {
            matched_steps: self.matched,
            differed_steps: self.differed,
            failed_steps: self.failed,
            caveats,
        }
    }
}

fn replay_step(
    step: &InvestigationStep,
    index_dir: &std::path::Path,
    replay: &mut ReplayAccumulator,
) {
    let replay_start = Instant::now();
    match step.step_kind.as_str() {
        "search" => replay_search_step(step, index_dir, replay_start, replay),
        other => replay.differed.push(StepReplayDifferDto {
            step_id: step.id.clone(),
            step_kind: step.step_kind.clone(),
            recorded_duration_ms: recorded_duration_ms(step),
            replay_duration_ms: replay_start.elapsed().as_millis() as u32,
            expected: format!("replay for step_kind '{other}'"),
            actual: "not yet replayable".to_string(),
        }),
    }
}

fn replay_search_step(
    step: &InvestigationStep,
    index_dir: &std::path::Path,
    replay_start: Instant,
    replay: &mut ReplayAccumulator,
) {
    let params: serde_json::Value = match serde_json::from_str(&step.params_json) {
        Ok(params) => params,
        Err(error) => {
            push_replay_failure(
                replay,
                step,
                format!("Failed to parse params_json: {error}"),
            );
            return;
        }
    };
    let query = params
        .get("query")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if query.is_empty() {
        push_replay_failure(
            replay,
            step,
            "Missing 'query' field in params_json".to_string(),
        );
        return;
    }
    match super::search_service::search_files_real(index_dir, query, 0, 50) {
        Ok(result) => push_replay_match(replay, step, query, result.total, replay_start),
        Err(error) => push_replay_failure(replay, step, format!("Search replay failed: {error}")),
    }
}

fn push_replay_match(
    replay: &mut ReplayAccumulator,
    step: &InvestigationStep,
    query: &str,
    total: u64,
    replay_start: Instant,
) {
    let replay_duration = replay_start.elapsed().as_millis() as u32;
    let recorded_duration = recorded_duration_ms(step);
    replay.matched.push(StepReplayMatchDto {
        step_id: step.id.clone(),
        step_kind: step.step_kind.clone(),
        recorded_duration_ms: recorded_duration,
        replay_duration_ms: replay_duration,
        detail: format!(
            "Replayed search for '{}': {} hits (recorded {} ms, replay {} ms)",
            query, total, recorded_duration, replay_duration,
        ),
    });
}

fn push_replay_failure(replay: &mut ReplayAccumulator, step: &InvestigationStep, error: String) {
    replay.failed.push(StepReplayFailDto {
        step_id: step.id.clone(),
        step_kind: step.step_kind.clone(),
        recorded_duration_ms: recorded_duration_ms(step),
        error,
    });
}

fn recorded_duration_ms(step: &InvestigationStep) -> u32 {
    step.duration_ms.unwrap_or(0) as u32
}

#[cfg(test)]
#[path = "../tests/unit/step_replay.rs"]
mod tests;
