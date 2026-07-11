//! Import command scheduling facade.

use app_services::{active_case, cluster_service, import_analysis, import_precheck};
use tauri::{AppHandle, State};
use transport::{commands::ImportDataSourceRequest, CommandError};

use crate::state::{AppState, TaskManager};

mod preparation;
mod queue;
mod request;

pub async fn import_data_source(
    state: State<'_, AppState>,
    app: AppHandle,
    request: ImportDataSourceRequest,
) -> Result<String, CommandError> {
    request::import_data_source(state, app, request).await
}

pub fn schedule_import_for_active_case(
    active: &active_case::ActiveCase,
    import_config: import_precheck::ImportSourceConfig,
    app: Option<&AppHandle>,
    task_manager: &TaskManager,
    max_import_workers: Option<usize>,
    max_analysis_workers: Option<usize>,
    analysis_mode: import_analysis::ImportAnalysisMode,
) -> Result<String, CommandError> {
    queue::schedule_import_for_active_case(
        active,
        import_config,
        app,
        task_manager,
        max_import_workers,
        max_analysis_workers,
        analysis_mode,
    )
}

pub fn schedule_linux_cluster_import_for_active_case(
    active: &active_case::ActiveCase,
    plan: cluster_service::LinuxClusterImportPlan,
    app: Option<&AppHandle>,
    task_manager: &TaskManager,
    max_import_workers: Option<usize>,
    max_analysis_workers: Option<usize>,
    analysis_mode: import_analysis::ImportAnalysisMode,
) -> Result<String, CommandError> {
    queue::schedule_linux_cluster_import_for_active_case(
        active,
        plan,
        app,
        task_manager,
        max_import_workers,
        max_analysis_workers,
        analysis_mode,
    )
}

pub(crate) fn import_config_error_to_command_error(
    error: import_precheck::ImportSourceConfigError,
) -> CommandError {
    preparation::map_import_config_error(error)
}

#[cfg(test)]
#[path = "../../../tests/unit/commands/import/schedule_test.rs"]
mod tests;
