use std::path::Path;

use domain::CaseId;
use persistence_sqlite::repositories::notebook_repo::NotebookRepo;
use rusqlite::Connection;
use transport::dto::InvestigationStepDto;

use super::Result;

pub struct LinuxImportPhaseInput<'a> {
    pub case_id: &'a CaseId,
    pub import_set_id: &'a str,
    pub phase: &'a str,
    pub success: bool,
    pub error: Option<&'a str>,
    pub ready_count: u32,
    pub failed_count: u32,
}

pub fn record_linux_import_phase(
    connection: &Connection,
    case_root: &Path,
    input: LinuxImportPhaseInput<'_>,
) -> Result<InvestigationStepDto> {
    let step_kind = "linux_evidence_set_import";
    let attempt = NotebookRepo::new(connection)
        .count_import_phase_steps(
            &input.case_id.0,
            step_kind,
            input.import_set_id,
            input.phase,
        )?
        .saturating_add(1);
    let params = serde_json::json!({
        "importSetId": input.import_set_id, "phase": input.phase, "attempt": attempt,
        "readyCount": input.ready_count, "failedCount": input.failed_count, "error": input.error,
    })
    .to_string();
    Ok(crate::step_recorder::record_step(
        connection,
        case_root,
        crate::step_recorder::CaseStepInput {
            case_id: &input.case_id.0,
            step_kind,
            params_json: &params,
            duration_ms: 0,
            success: input.success,
            error_code: input.error,
        },
    )?)
}
