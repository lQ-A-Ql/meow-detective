use persistence_sqlite::repositories::notebook_repo::{
    InvestigationStep, NotebookRepo, StepFilters,
};
use rusqlite::Connection;
use transport::dto::InvestigationStepDto;
use uuid::Uuid;

use super::dto_conversion::step_to_dto;
use super::NotebookError;

/// Record an investigation step for audit/replay purposes.
///
/// Returns the recorded step as a DTO.
#[allow(clippy::too_many_arguments)]
pub fn record_step(
    conn: &Connection,
    case_id: &str,
    step_kind: &str,
    params_json: &str,
    timestamp: &str,
    duration_ms: u32,
    success: bool,
    error_code: Option<&str>,
    case_state_hash: Option<&str>,
) -> Result<InvestigationStepDto, NotebookError> {
    let step = InvestigationStep {
        id: Uuid::new_v4().to_string(),
        case_id: case_id.to_string(),
        step_kind: step_kind.to_string(),
        params_json: params_json.to_string(),
        timestamp: timestamp.to_string(),
        duration_ms: Some(duration_ms as i64),
        case_state_hash: case_state_hash.map(str::to_string),
        success: Some(success),
        error_code: error_code.map(str::to_string),
    };

    NotebookRepo::new(conn).record_step(&step)?;

    Ok(step_to_dto(&step))
}

/// List investigation steps for a case, with optional filters.
pub fn list_steps(
    conn: &Connection,
    case_id: &str,
    filters: &StepFilters,
) -> Result<Vec<InvestigationStepDto>, NotebookError> {
    let steps = NotebookRepo::new(conn).list_steps(case_id, filters)?;
    Ok(steps.iter().map(step_to_dto).collect())
}
