use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::analysis_service::run_source_analysis_extraction_with_cancel;
use app_services::cluster_service::{collect_linux_evidence_facts, persist_linux_evidence_facts};
use domain::{CaseId, DataSourceId};
use tauri::AppHandle;
use transport::CommandError;

use crate::events::event_bridge;
use crate::state::{TaskManager, TaskRegistrationError, TaskScope};

const LINUX_ARTIFACT_TASK_PREFIX: &str = "linux-artifacts:";

/// Schedule Linux artifact extraction after a cluster member Catalog is ready.
///
/// The import pipeline intentionally does not await this work for evidence-set
/// members. Extraction itself retains the process-wide serial gate, while the
/// next member can continue its independent E01/BlueStore import and hashing.
pub(crate) fn schedule_linux_artifact_analysis(
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    app: Option<&AppHandle>,
    task_manager: Arc<TaskManager>,
) -> Result<bool, CommandError> {
    let db_path = case_root.join("app.db");
    let connection = app_services::connection::open_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    if !app_services::linux_artifact_job::source_is_ready_linux(&connection, data_source_id)
        .map_err(CommandError::from_typed_service_error)?
    {
        return Ok(false);
    }

    let task_id = format!("{LINUX_ARTIFACT_TASK_PREFIX}{}", data_source_id.0);
    if task_manager.is_running(&task_id) {
        return Ok(false);
    }
    let job_id = app_services::linux_artifact_job::create_job(&connection, case_id)
        .map_err(CommandError::from_typed_service_error)?;
    let job_id_value = job_id.0.clone();
    app_services::linux_artifact_job::update_progress(
        &connection,
        &job_id,
        1,
        "Queued Linux artifact analysis",
    )
    .map_err(CommandError::from_typed_service_error)?;
    let scope = TaskScope::data_source(&case_id.0, &data_source_id.0, &task_id);
    let case_root = case_root.to_path_buf();
    let case_id = case_id.clone();
    let data_source_id = data_source_id.clone();
    let app = app.cloned();
    let worker_task_id = task_id.clone();
    let cancel_token = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel_token);
    let registration = task_manager.spawn_scoped(task_id.clone(), scope, cancel_token, move || {
        run_linux_artifact_analysis(
            &case_root,
            &case_id,
            &data_source_id,
            app.as_ref(),
            worker_cancel,
            &worker_task_id,
            &job_id_value,
        )
    });
    registration.map_err(|error| {
        let detail = match &error {
            TaskRegistrationError::DuplicateTaskId(_) => CommandError::conflict(
                "Linux artifact analysis is already scheduled for this source",
            ),
            other => CommandError::internal(format!(
                "Linux artifact analysis task registration failed: {other}"
            )),
        };
        if let Ok(connection) = app_services::connection::open_case_db(&db_path) {
            let _ = app_services::linux_artifact_job::fail(&connection, &job_id, &detail.message);
        }
        detail
    })?;
    Ok(true)
}

fn run_linux_artifact_analysis(
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
    task_id: &str,
    job_id: &str,
) -> Result<(), String> {
    if let Some(app) = app {
        event_bridge::emit_job_created(app, job_id, "Linux artifact analysis");
        event_bridge::emit_job_started(app, task_id, "Linux artifact analysis started");
        event_bridge::emit_job_progress(app, task_id, 1, "Linux artifact analysis queued");
    }
    let connection = app_services::connection::open_case_db(&case_root.join("app.db"))
        .map_err(|error| error.to_string())?;
    let job_id = domain::JobId(job_id.to_string());
    app_services::linux_artifact_job::update_progress(
        &connection,
        &job_id,
        5,
        "Linux artifact analysis started",
    )
    .map_err(|error| error.to_string())?;
    let result = run_source_analysis_extraction_with_cancel(
        &connection,
        case_root,
        case_id,
        data_source_id,
        &["LinuxArtifacts"],
        cancel_token,
    );
    match result {
        Ok(run) => {
            let facts =
                collect_linux_evidence_facts(&connection, case_root, case_id, data_source_id);
            let source = app_services::source_db::open_registered_source_db(
                &connection,
                case_root,
                data_source_id,
            )
            .map_err(|error| error.to_string())?;
            persist_linux_evidence_facts(&source, data_source_id, &facts)
                .map_err(|error| error.to_string())?;
            let detail = format!(
                "Linux artifact analysis completed: artifacts={} timeline={}",
                run.artifact_count, run.timeline_event_count
            );
            if let Some(app) = app {
                event_bridge::emit_job_progress(app, task_id, 100, &detail);
                event_bridge::emit_job_completed(app, task_id, &detail);
            }
            app_services::linux_artifact_job::complete(&connection, &job_id, &detail)
                .map_err(|error| error.to_string())?;
            Ok(())
        }
        Err(error) => {
            let detail = error.to_string();
            if let Some(app) = app {
                event_bridge::emit_job_failed(app, task_id, &detail);
            }
            let _ = app_services::linux_artifact_job::fail(&connection, &job_id, &detail);
            Err(detail)
        }
    }
}
