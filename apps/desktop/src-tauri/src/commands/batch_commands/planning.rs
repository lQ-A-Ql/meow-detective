use tauri::State;
use transport::dto::batch::{BatchJobDto, BatchPlanDto, BatchResourceLimitsDto};
use transport::CommandError;

use crate::commands::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

#[tauri::command]
pub async fn create_batch_plan(
    state: State<'_, AppState>,
    name: String,
    data_source_ids: Vec<String>,
    phases: Vec<String>,
    resource_limits: BatchResourceLimitsDto,
) -> Result<BatchJobDto, CommandError> {
    create_batch_plan_impl(
        state.inner(),
        name,
        data_source_ids,
        phases,
        resource_limits,
    )
    .await
}

pub(super) async fn create_batch_plan_impl(
    app_state: &AppState,
    name: String,
    data_source_ids: Vec<String>,
    phases: Vec<String>,
    resource_limits: BatchResourceLimitsDto,
) -> Result<BatchJobDto, CommandError> {
    if name.trim().is_empty() || name.len() > 200 {
        return Err(CommandError::invalid_input(
            "Batch name must be 1-200 characters",
        ));
    }
    if data_source_ids.is_empty() {
        return Err(CommandError::invalid_input(
            "At least one data source is required",
        ));
    }
    if phases.is_empty() {
        return Err(CommandError::invalid_input(
            "At least one phase is required",
        ));
    }

    let app_state = app_state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_connection(&app_state)?;
        let plan = BatchPlanDto {
            data_source_refs: data_source_ids,
            phases,
            resource_limits,
        };
        app_services::batch_service::create_and_persist_batch(
            &connection,
            &active.case_id,
            &name,
            plan,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
