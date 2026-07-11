use app_services::cluster_service;
use tauri::{AppHandle, State};
use transport::{
    commands::{ImportDataSourceRequest, ImportSourceKindDto},
    CommandError,
};

use super::{
    preparation::{
        import_analysis_mode_from_settings, load_import_settings, prepare_import_config,
        validate_import_request,
    },
    schedule_import_for_active_case, schedule_linux_cluster_import_for_active_case,
};
use crate::state::AppState;

pub async fn import_data_source(
    state: State<'_, AppState>,
    app: AppHandle,
    request: ImportDataSourceRequest,
) -> Result<String, CommandError> {
    let app_state = state.inner().clone();
    let source_path = request.source_path.clone();
    let app_clone = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let platform = validate_import_request(&request)?;
        let settings = load_import_settings(&app_state.app_settings_path);
        let guard = app_state
            .active_case
            .lock()
            .map_err(|error| CommandError::from_lock_error("Case", error))?;
        let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
        let analysis_mode = import_analysis_mode_from_settings(&settings.import_analysis_mode);
        let _job_id = match request.source_kind {
            ImportSourceKindDto::LinuxCluster => {
                let plan = cluster_service::plan_linux_cluster_import(
                    &request.source_path,
                    request.profile.clone(),
                )
                .map_err(CommandError::from_typed_service_error)?;
                schedule_linux_cluster_import_for_active_case(
                    active,
                    plan,
                    Some(&app_clone),
                    &app_state.task_manager,
                    settings.max_import_workers,
                    settings.max_analysis_workers,
                    analysis_mode,
                )?
            }
            ImportSourceKindDto::Auto => {
                let import_config = prepare_import_config(&request, platform)?;
                schedule_import_for_active_case(
                    active,
                    import_config,
                    Some(&app_clone),
                    &app_state.task_manager,
                    settings.max_import_workers,
                    settings.max_analysis_workers,
                    analysis_mode,
                )?
            }
        };
        Ok(format!(
            "Import started for {}. Watch the Jobs panel for progress.",
            source_path
        ))
    })
    .await
    .map_err(CommandError::from_join_error)?
}
