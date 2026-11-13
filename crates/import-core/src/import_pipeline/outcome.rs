use crate::import_precheck::ImportSourceConfig;
use crate::step_recorder::{self, CaseStepInput};
use std::{path::Path, time::Duration};
use transport::CommandError;

pub(super) fn parameters(config: &ImportSourceConfig) -> String {
    serde_json::json!({
        "sourcePath": config.source_path_display,
        "sourceName": config.source_name,
        "kind": format!("{:?}", config.kind),
    })
    .to_string()
}

pub(super) fn record<T>(
    connection: &rusqlite::Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    parameters: &str,
    elapsed: Duration,
    result: &Result<T, CommandError>,
) -> Result<(), CommandError> {
    step_recorder::record_step(
        connection,
        case_root,
        CaseStepInput {
            case_id: &case_id.0,
            step_kind: "import",
            params_json: parameters,
            duration_ms: elapsed.as_millis().min(u32::MAX as u128) as u32,
            success: result.is_ok(),
            error_code: result.as_ref().err().map(|error| error.code.as_str()),
        },
    )
    .map_err(CommandError::from_typed_service_error)?;
    Ok(())
}
