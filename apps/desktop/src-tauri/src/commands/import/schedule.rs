//! Import command scheduling and request preparation.

use app_services::{active_case, import_analysis, import_precheck};
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::{AppHandle, State};
use transport::{
    commands::{AppSettingsDto, ImportDataSourceRequest},
    CommandError,
};

use crate::events::event_bridge;
use crate::state::AppState;

use super::background_job::{run_background_import_job, BackgroundImportJob};

/// Tauri command: Import a data source into the current case.
pub async fn import_data_source(
    state: State<'_, AppState>,
    app: AppHandle,
    request: ImportDataSourceRequest,
) -> Result<String, CommandError> {
    let import_config = prepare_import_config(&request)?;
    let app_state = state.inner().clone();
    let source_path = import_config.source_path_display.clone();
    let app_clone = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_import_settings(&app_state.app_settings_path);
        let guard = app_state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
        let _job_id_str = schedule_import_for_active_case(
            active,
            import_config,
            Some(&app_clone),
            &app_state.task_manager,
            settings.max_import_workers,
            settings.max_analysis_workers,
            import_analysis_mode_from_settings(&settings.import_analysis_mode),
        )?;
        Ok(format!(
            "Import started for {}. Watch the Jobs panel for progress.",
            source_path
        ))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn load_import_settings(path: &std::path::Path) -> AppSettingsDto {
    match std::fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<AppSettingsDto>(&raw) {
            Ok(settings) => {
                if let Err(error) = settings.validate() {
                    tracing::warn!(
                        "Ignoring invalid app settings at {}: {}",
                        path.display(),
                        error
                    );
                    AppSettingsDto::default()
                } else {
                    settings
                }
            }
            Err(error) => {
                tracing::warn!(
                    "Ignoring unreadable app settings at {}: {}",
                    path.display(),
                    error
                );
                AppSettingsDto::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => AppSettingsDto::default(),
        Err(error) => {
            tracing::warn!(
                "Ignoring app settings read error at {}: {}",
                path.display(),
                error
            );
            AppSettingsDto::default()
        }
    }
}

/// Schedule an import job for the active case.
pub fn schedule_import_for_active_case(
    active: &active_case::ActiveCase,
    import_config: import_precheck::ImportSourceConfig,
    app: Option<&AppHandle>,
    task_manager: &crate::state::TaskManager,
    max_import_workers: Option<usize>,
    max_analysis_workers: Option<usize>,
    analysis_mode: import_analysis::ImportAnalysisMode,
) -> Result<String, CommandError> {
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();
    let db_path = active.db_path();
    let source_name = import_config.source_name.clone();

    let conn = app_services::connection::open_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&conn);
    let job_id = job_repo
        .create(&case_id.0, "Import data source")
        .map_err(CommandError::from_typed_service_error)?;
    let job_id_str = job_id.0.clone();
    if let Some(app) = app {
        event_bridge::emit_job_created(app, &job_id_str, "Import data source");
    }
    job_repo
        .update_progress(&job_id, 1, &format!("Queued import for {source_name}"))
        .map_err(CommandError::from_typed_service_error)?;

    let app_handle = app.cloned();
    let cancel_token = Arc::new(AtomicBool::new(false));
    let cancel_token_clone = cancel_token.clone();

    let _job_id_clone = job_id.clone();
    let background_job = BackgroundImportJob {
        db_path,
        case_id,
        case_root,
        import_config,
        job_id,
        max_import_workers,
        max_analysis_workers,
        analysis_mode,
    };
    let handle = std::thread::spawn(move || {
        run_background_import_job(background_job, app_handle.as_ref(), cancel_token_clone)
            .map_err(|e| e.message)
    });

    // Register with TaskManager using the cancel token
    task_manager.register_with_token(job_id_str.clone(), handle, cancel_token);

    Ok(job_id_str)
}

fn import_analysis_mode_from_settings(value: &str) -> import_analysis::ImportAnalysisMode {
    match value {
        "budgetedContent" => import_analysis::ImportAnalysisMode::BudgetedContent,
        "fullContent" => import_analysis::ImportAnalysisMode::FullContent,
        _ => import_analysis::ImportAnalysisMode::MetadataOnly,
    }
}

fn prepare_import_config(
    request: &ImportDataSourceRequest,
) -> Result<import_precheck::ImportSourceConfig, CommandError> {
    import_precheck::prepare_import_source_config(request)
        .map_err(import_config_error_to_command_error)
}

pub(crate) fn import_config_error_to_command_error(
    error: import_precheck::ImportSourceConfigError,
) -> CommandError {
    if error.is_invalid_input() {
        CommandError::invalid_input(error.to_string())
    } else {
        CommandError::from_typed_service_error(error)
    }
}
