use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use app_services::{active_case, cluster_service, import_analysis};
use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::CommandError;

use super::super::background_job::{
    cancel_browseable_cluster_job, complete_browseable_cluster_job,
    continue_cluster_rbd_processing, fail_browseable_cluster_job,
    run_background_linux_cluster_import_until_browseable, BackgroundDerivedSourceProcessingJob,
    BackgroundLinuxClusterImportJob,
};
use crate::events::event_bridge;
use crate::state::TaskScope;

pub(super) fn schedule_linux_cluster_import_for_active_case(
    active: &active_case::ActiveCase,
    plan: cluster_service::LinuxClusterImportPlan,
    app: Option<&AppHandle>,
    task_manager: Arc<crate::state::TaskManager>,
    max_import_workers: Option<usize>,
    max_analysis_workers: Option<usize>,
    analysis_mode: import_analysis::ImportAnalysisMode,
) -> Result<String, CommandError> {
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();
    let db_path = active.db_path();
    let cluster_name = plan.cluster_name.clone();
    let member_count = plan.members.len();
    let connection = app_services::connection::open_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&connection);
    let job_id = job_repo
        .create(&case_id.0, "Import Linux cluster")
        .map_err(CommandError::from_typed_service_error)?;
    let job_id_string = job_id.0.clone();
    if let Some(app) = app {
        event_bridge::emit_job_created(app, &job_id_string, "Import Linux cluster");
    }
    job_repo
        .update_progress(
            &job_id,
            1,
            &format!("Queued Linux cluster import for {cluster_name} ({member_count} images)"),
        )
        .map_err(CommandError::from_typed_service_error)?;

    let app_handle = app.cloned();
    let cancel_token = Arc::new(AtomicBool::new(false));
    let background_cancel_token = cancel_token.clone();
    let processing_task_manager = task_manager.clone();
    let processing_group_id = job_id_string.clone();
    let background_job = BackgroundLinuxClusterImportJob {
        db_path,
        case_id,
        case_root,
        plan,
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
            let processing = run_background_linux_cluster_import_until_browseable(
                background_job,
                app_handle.as_ref(),
                background_cancel_token.clone(),
            )
            .map_err(|error| error.message)?;
            if let Some(outcome) = processing {
                if background_cancel_token.load(Ordering::Acquire) {
                    cancel_browseable_cluster_job(
                        &outcome,
                        app_handle.as_ref(),
                        "Linux cluster import cancelled before derived processing admission",
                    );
                    return Ok(());
                }
                for data_source_id in outcome.processing.source_ids.iter().cloned() {
                    if background_cancel_token.load(Ordering::Acquire) {
                        processing_task_manager.cancel(&processing_group_id);
                        cancel_browseable_cluster_job(
                            &outcome,
                            app_handle.as_ref(),
                            "Linux cluster import cancelled during derived processing admission",
                        );
                        return Ok(());
                    }
                    if let Err(error) = schedule_derived_processing(
                        &processing_task_manager,
                        &processing_group_id,
                        &outcome.processing,
                        data_source_id,
                    ) {
                        processing_task_manager.cancel(&processing_group_id);
                        fail_browseable_cluster_job(&outcome, app_handle.as_ref(), &error.message);
                        return Err(error.message);
                    }
                }
                complete_browseable_cluster_job(&outcome, app_handle.as_ref())
                    .map_err(|error| error.message)?;
            }
            Ok(())
        },
    ) {
        let detail = format!("Cluster import task registration failed: {error}");
        let _ = job_repo.fail(&domain::JobId(job_id_string.clone()), &detail);
        return Err(CommandError::internal(detail));
    }
    Ok(job_id_string)
}

fn schedule_derived_processing(
    task_manager: &crate::state::TaskManager,
    group_id: &str,
    processing: &BackgroundDerivedSourceProcessingJob,
    data_source_id: domain::DataSourceId,
) -> Result<(), CommandError> {
    let data_source_id_value = data_source_id.0.clone();
    let task_id = format!("{group_id}:derived-processing:{data_source_id_value}");
    let scope = TaskScope::data_source(
        processing.case_id.0.clone(),
        data_source_id_value.clone(),
        group_id,
    );
    let job = BackgroundDerivedSourceProcessingJob {
        db_path: processing.db_path.clone(),
        case_id: processing.case_id.clone(),
        case_root: processing.case_root.clone(),
        cluster_id: processing.cluster_id.clone(),
        source_ids: vec![data_source_id],
    };
    let cancel_token = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel_token);
    task_manager
        .spawn_scoped_heavy(task_id.clone(), scope, cancel_token, move || {
            continue_cluster_rbd_processing(&job, &worker_cancel).map_err(|error| {
                tracing::warn!(
                    cluster_id = %job.cluster_id,
                    data_source_id = data_source_id_value,
                    error = %error.message,
                    "Managed derived-source background processing stopped"
                );
                error.message
            })
        })
        .map_err(|error| {
            CommandError::internal(format!(
                "Derived-source processing task '{task_id}' was not admitted: {error}"
            ))
        })
}
