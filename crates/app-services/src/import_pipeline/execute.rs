use std::sync::atomic::Ordering;
use std::time::Instant;

use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use transport::{dto::CancellationStateDto, CommandError};

use crate::import_pipeline::{
    context::ImportJobContext,
    emit::ImportEventSink,
    options::{ImportJobOptions, JobOutcomeCounts},
    phases, profile,
};
use crate::import_precheck;

/// Execute the import job (main logic).
pub fn execute_import_job(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    case_root: &std::path::Path,
    import_config: import_precheck::ImportSourceConfig,
    job_id: &domain::JobId,
    options: ImportJobOptions<'_>,
) -> Result<String, CommandError> {
    let (message, _counts) =
        execute_import_job_with_counts(conn, case_id, case_root, import_config, job_id, options)?;
    Ok(message)
}

/// Execute the import job and return both the summary message and outcome counts.
pub fn execute_import_job_with_counts(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    case_root: &std::path::Path,
    import_config: import_precheck::ImportSourceConfig,
    job_id: &domain::JobId,
    options: ImportJobOptions<'_>,
) -> Result<(String, JobOutcomeCounts), CommandError> {
    let job_repo = JobRepo::new(conn);
    let mut counts = JobOutcomeCounts::default();
    let import_started = Instant::now();
    let source_conn: Option<rusqlite::Connection> = None;
    let source_path_display = import_config.source_path_display.clone();

    let mut ctx = ImportJobContext {
        conn,
        source_conn: source_conn.as_ref(),
        case_id,
        case_root,
        source_path: &source_path_display,
        job_id,
        options,
        import_config,
        ds: None,
        job_repo,
        counts: &mut counts,
    };

    let ds = phases::run_attach_phase(&mut ctx)?;
    let source_conn = crate::source_db::open_source_db(case_root, &ds.id)
        .map_err(CommandError::from_service_error)?;
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(case_id, &ds)
        .map_err(CommandError::from_service_error)?;
    DataSourceRepo::new(conn)
        .update_import_state(&ds.id, "importing", None)
        .map_err(CommandError::from_service_error)?;
    ctx.source_conn = Some(&source_conn);
    ctx.ds = Some(&ds);

    let result = (|| {
        reject_cancelled_after_register(&ctx, &ds)?;

        let stats = phases::run_enumeration_phase(&mut ctx, &ds)?;
        let pipeline_msg = phases::run_analyze_phase(&mut ctx, &ds)?;
        phases::run_finalize_phase(&mut ctx, &ds, &stats, &pipeline_msg, import_started)
    })();

    persist_import_outcome(conn, &ds.id, result).map(|message| (message, counts))
}

fn reject_cancelled_after_register(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<(), CommandError> {
    if !ctx.options.cancel_token.load(Ordering::Relaxed) {
        return Ok(());
    }
    mark_import_cancelling(
        &ctx.job_repo,
        ctx.job_id,
        "Cancellation acknowledged after attach",
    );
    emit_import_cancellation_state(
        ctx.event_sink(),
        ctx.job_id,
        CancellationStateDto::Acknowledged,
        false,
        "Cancellation acknowledged after attach",
    );
    profile::emit_import_profile_progress(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        12,
        "Cancellation acknowledged: phase=attach",
        true,
    );
    Err(CommandError::internal("Import cancelled by user"))
}

fn persist_import_outcome(
    conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    result: Result<String, CommandError>,
) -> Result<String, CommandError> {
    match result {
        Ok(message) => {
            DataSourceRepo::new(conn)
                .update_import_state(data_source_id, "ready", None)
                .map_err(CommandError::from_service_error)?;
            Ok(message)
        }
        Err(error) => {
            persist_failed_import(conn, data_source_id, &error);
            Err(error)
        }
    }
}

fn persist_failed_import(
    conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    error: &CommandError,
) {
    if let Err(update_error) = DataSourceRepo::new(conn).update_import_state(
        data_source_id,
        "failed",
        Some(&error.message),
    ) {
        tracing::warn!(
            data_source_id = %data_source_id.0,
            error = %update_error,
            "Failed to persist data source import failure state"
        );
    }
}

pub(crate) fn emit_import_cancellation_state(
    event_sink: Option<&dyn ImportEventSink>,
    job_id: &domain::JobId,
    state: CancellationStateDto,
    safe_to_close: bool,
    detail: &str,
) {
    crate::import_pipeline::emit::emit_job_cancellation(
        event_sink,
        &job_cancellation_dto(&job_id.0, state, safe_to_close, detail),
    );
}

pub(crate) fn job_cancellation_dto(
    job_id: &str,
    state: CancellationStateDto,
    safe_to_close: bool,
    detail: &str,
) -> transport::dto::JobCancellationDto {
    let now = chrono::Utc::now().to_rfc3339();
    transport::dto::JobCancellationDto {
        job_id: job_id.to_string(),
        requested_at: Some(now.clone()),
        acknowledged_at: matches!(
            state,
            CancellationStateDto::Acknowledged
                | CancellationStateDto::Draining
                | CancellationStateDto::Cancelled
                | CancellationStateDto::TimedOut
        )
        .then_some(now),
        state,
        safe_to_close,
        detail: detail.to_string(),
    }
}

pub(crate) fn mark_import_cancelling(job_repo: &JobRepo<'_>, job_id: &domain::JobId, detail: &str) {
    if let Err(error) = job_repo.mark_cancelling(job_id, detail) {
        tracing::warn!("Failed to mark job {} as cancelling: {}", job_id.0, error);
    }
}

pub(crate) fn is_import_cancelled_message(message: &str) -> bool {
    message.to_ascii_lowercase().contains("cancel")
}
