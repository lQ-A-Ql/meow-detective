use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::{active_case, import_analysis, import_precheck};
use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::CommandError;

use super::super::background_job::{run_background_import_job, BackgroundImportJob};
use crate::events::event_bridge;
use crate::state::TaskScope;

pub fn schedule_import_for_active_case(
    active: &active_case::ActiveCase,
    import_config: import_precheck::ImportSourceConfig,
    app: Option<&AppHandle>,
    task_manager: Arc<crate::state::TaskManager>,
    max_import_workers: Option<usize>,
    max_analysis_workers: Option<usize>,
    analysis_mode: import_analysis::ImportAnalysisMode,
) -> Result<String, CommandError> {
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();
    let db_path = active.db_path();
    let source_name = import_config.source_name.clone();
    let connection = app_services::connection::open_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&connection);
    let job_id = job_repo
        .create(&case_id.0, "Import data source")
        .map_err(CommandError::from_typed_service_error)?;
    let job_id_string = job_id.0.clone();
    if let Some(app) = app {
        event_bridge::emit_job_created(app, &job_id_string, "Import data source");
    }
    job_repo
        .update_progress(&job_id, 1, &format!("Queued import for {source_name}"))
        .map_err(CommandError::from_typed_service_error)?;

    let app_handle = app.cloned();
    let cancel_token = Arc::new(AtomicBool::new(false));
    let background_cancel_token = cancel_token.clone();
    let task_manager_for_body = Arc::clone(&task_manager);
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
    if let Err(error) = task_manager.spawn_scoped(
        job_id_string.clone(),
        TaskScope::case(active.meta.id.0.clone(), job_id_string.clone()),
        cancel_token,
        move || {
            run_background_import_job(
                background_job,
                app_handle.as_ref(),
                background_cancel_token,
                task_manager_for_body,
            )
            .map_err(|error| error.message)
        },
    ) {
        let detail = format!("Import task registration failed: {error}");
        let _ = job_repo.fail(&domain::JobId(job_id_string.clone()), &detail);
        return Err(CommandError::internal(detail));
    }
    Ok(job_id_string)
}
