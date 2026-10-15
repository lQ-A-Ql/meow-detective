use app_services::{
    evidence_tasks,
    import_scheduler::{global_import_admission, ImportAdmissionRequest},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::AppHandle;
use transport::{
    dto::{EvidenceTaskRequestDto, EvidenceTaskResultDto},
    CommandError,
};

pub(super) struct TaskContext {
    pub root: PathBuf,
    pub db_path: PathBuf,
    pub case: domain::CaseId,
    pub app: AppHandle,
    pub plan: evidence_tasks::EvidenceTaskPlan,
    pub registry: Arc<app_services::registry_browser_service::RegistryBrowserRegistry>,
    pub bitlocker: Arc<app_services::bitlocker_runtime::BitLockerUnlockRegistry>,
    pub preview: Arc<app_services::file_service::PreviewRuntimeRegistry>,
}
pub(super) fn run(
    ctx: TaskContext,
    cancel: Arc<AtomicBool>,
) -> crate::state::task_manager::TaskResult {
    let conn =
        app_services::connection::open_case_db(&ctx.db_path).map_err(|error| error.to_string())?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute(&ctx, &conn, &cancel)
    }))
    .unwrap_or_else(|_| Err(CommandError::internal("Evidence task failed")));
    let success = result.is_ok();
    let cancelled = result
        .as_ref()
        .err()
        .is_some_and(|error| error.category == "cancelled");
    evidence_tasks::finish(&conn, &ctx.case, &ctx.plan.task.task_id, result)
        .map_err(|error| error.message)?;
    let events = &ctx.app;
    let id = &ctx.plan.task.task_id;
    if success {
        crate::events::event_bridge::emit_job_completed(events, id, "Evidence task completed");
    } else if cancelled {
        crate::events::event_bridge::emit_job_cancelled(events, id, "Evidence task cancelled");
    } else {
        crate::events::event_bridge::emit_job_failed(events, id, "Evidence task failed");
    }
    Ok(())
}
fn execute(
    ctx: &TaskContext,
    conn: &rusqlite::Connection,
    cancel: &Arc<AtomicBool>,
) -> Result<EvidenceTaskResultDto, CommandError> {
    let _admission = global_import_admission()
        .acquire(
            ImportAdmissionRequest {
                cpu_weight: 1,
                memory_mb: 256,
            },
            cancel,
        )
        .map_err(|_| CommandError::cancelled("Evidence task cancelled"))?;
    if cancel.load(Ordering::Acquire) {
        return Err(CommandError::cancelled("Evidence task cancelled"));
    }
    if evidence_tasks::revision(conn, &ctx.root, &ctx.case, &ctx.plan.request)?
        != ctx.plan.task.source_revision
    {
        return Err(CommandError::security(
            "Evidence changed while task was queued",
        ));
    }
    let clock = Mutex::new(Instant::now() - Duration::from_secs(1));
    let progress = |bytes, total| {
        let mut last = clock.lock().unwrap_or_else(|error| error.into_inner());
        if last.elapsed() < Duration::from_millis(250) {
            return;
        }
        *last = Instant::now();
        if let Ok(conn) = app_services::connection::open_case_db(&ctx.db_path) {
            if let Ok(percent) =
                evidence_tasks::progress(&conn, &ctx.case, &ctx.plan.task.task_id, bytes, total)
            {
                crate::events::event_bridge::emit_job_progress(
                    &ctx.app,
                    &ctx.plan.task.task_id,
                    percent,
                    "Processing evidence",
                );
            }
        }
    };
    dispatch(ctx, conn, cancel, &progress)
}

fn dispatch(
    ctx: &TaskContext,
    conn: &rusqlite::Connection,
    cancel: &Arc<AtomicBool>,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<EvidenceTaskResultDto, CommandError> {
    match &ctx.plan.request {
        EvidenceTaskRequestDto::Digest(request) => {
            app_services::digest_service::calculate_evidence_digest_with_cancel(
                conn,
                &ctx.root,
                &ctx.case,
                request.scope,
                request.algorithm,
                app_services::digest_service::DigestTarget {
                    data_source_id: request.data_source_id.as_deref(),
                    file_id: request.file_id.as_deref(),
                    partition_index: request.partition_index,
                },
                app_services::digest_service::DigestExecution {
                    cancelled: cancel,
                    progress: &progress,
                },
            )
            .map(EvidenceTaskResultDto::Digest)
            .map_err(CommandError::from_typed_service_error)
        }
        EvidenceTaskRequestDto::NestedImport(request) => {
            execute_nested(ctx, conn, cancel, &progress, request)
        }
        EvidenceTaskRequestDto::ContentIndex { data_source_id } => {
            app_services::content_index_service::build(
                conn,
                &ctx.root,
                &ctx.case,
                data_source_id,
                cancel,
                &progress,
            )
            .map(EvidenceTaskResultDto::ContentIndex)
        }
        EvidenceTaskRequestDto::RegistrySearch {
            file_id,
            session_id,
            query,
        } => {
            let context = app_services::registry_browser_service::RegistryBrowserContext {
                case_conn: conn,
                case_root: &ctx.root,
                case_id: &ctx.case,
                bitlocker: &ctx.bitlocker,
                preview: &ctx.preview,
            };
            let source = app_services::registry_browser_service::source_for_session(
                &ctx.registry,
                &context,
                session_id,
            )
            .map_err(CommandError::from_typed_service_error)?;
            if source.file_id != *file_id {
                return Err(CommandError::security(
                    "Registry search source differs from its session",
                ));
            }
            app_services::registry_browser_service::search_hive_with_cancel(
                &ctx.registry,
                &context,
                transport::dto::RegistrySearchRequestDto {
                    session_id: session_id.clone(),
                    query: query.clone(),
                },
                cancel,
                &progress,
            )
            .map(EvidenceTaskResultDto::RegistrySearch)
            .map_err(CommandError::from_typed_service_error)
        }
    }
}

fn execute_nested(
    ctx: &TaskContext,
    conn: &rusqlite::Connection,
    cancel: &Arc<AtomicBool>,
    progress: &(dyn Fn(u64, u64) + Sync),
    request: &transport::commands::MaterializeNestedEvidenceRequest,
) -> Result<EvidenceTaskResultDto, CommandError> {
    let mut result = app_services::nested_evidence_service::materialize_fixed_vhd_with_cancel(
        conn,
        &ctx.root,
        &ctx.case,
        &request.parent_data_source_id,
        &request.file_entry_id,
        cancel,
        progress,
    )
    .map_err(CommandError::from_typed_service_error)?;
    let source = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
        .find_by_case(&ctx.case)
        .map_err(CommandError::from_service_error)?
        .into_iter()
        .find(|source| source.id.0 == result.derived_data_source_id)
        .ok_or_else(|| CommandError::not_found("Derived source"))?;
    let storage = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
        .find_storage(&source.id)
        .map_err(CommandError::from_service_error)?
        .ok_or_else(|| CommandError::not_found("Derived source storage"))?;
    if storage.import_state == "ready" {
        result.import_state = "ready".into();
        result.import_job_id = Some(ctx.plan.task.task_id.clone());
        return Ok(EvidenceTaskResultDto::NestedImport(result));
    }
    let platform = storage
        .platform
        .parse::<domain::DataSourcePlatform>()
        .map_err(CommandError::from_service_error)?;
    let mut config = app_services::import_precheck::prepare_import_source_config_from_path(
        &source.source_path.to_string_lossy(),
        platform,
    )
    .map_err(CommandError::from_typed_service_error)?;
    config.existing_data_source_id = Some(source.id);
    let sink = crate::commands::import::events::TauriImportEventSink::new(&ctx.app);
    app_services::import_pipeline::execute_import_job(
        conn,
        &ctx.case,
        &ctx.root,
        config,
        &domain::JobId(ctx.plan.task.task_id.clone()),
        app_services::import_pipeline::ImportJobOptions {
            event_sink: Some(&sink),
            cancel_token: cancel,
            max_import_workers: Some(1),
            max_analysis_workers: Some(1),
            analysis_mode: app_services::import_analysis::ImportAnalysisMode::MetadataOnly,
            defer_linux_artifacts: false,
        },
    )?;
    result.import_state = "ready".into();
    result.import_job_id = Some(ctx.plan.task.task_id.clone());
    Ok(EvidenceTaskResultDto::NestedImport(result))
}
