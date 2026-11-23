use super::{
    context::ImportJobContext, execute::execute_observed_import_job, profile, ImportJobOptions,
    JobOutcomeCounts, PostImportMode,
};
use crate::import_precheck;
use std::time::Instant;
use transport::dto::import_benchmark::ImportBenchmarkPhaseDto as Phase;
use transport::{dto::TaskProgressStateDto as State, CommandError};

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_import_job_internal(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    case_root: &std::path::Path,
    import_config: import_precheck::ImportSourceConfig,
    job_id: &domain::JobId,
    options: ImportJobOptions<'_>,
    post_import_mode: PostImportMode,
    profiler: Option<super::ImportProfiler>,
) -> Result<(String, JobOutcomeCounts), CommandError> {
    let observer = super::progress::ObservedImportSink::new(
        options.event_sink,
        &job_id.0,
        &case_id.0,
        import_config.kind == domain::DataSourceKind::NestedRange,
    );
    std::thread::scope(|scope| {
        let (stop, receiver) = std::sync::mpsc::channel::<()>();
        let observer_ref = &observer;
        let cancel = options.cancel_token;
        let worker = scope.spawn(move || {
            while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                receiver.recv_timeout(std::time::Duration::from_secs(2))
            {
                observer_ref.heartbeat(cancel);
            }
        });
        let result = execute_observed_import_job(
            conn,
            case_id,
            case_root,
            import_config,
            job_id,
            ImportJobOptions {
                event_sink: Some(&observer),
                ..options
            },
            post_import_mode,
            profiler,
        );
        drop(stop);
        worker.join().expect("import heartbeat worker");
        let state = match &result {
            Ok(_) => State::Completed,
            Err(error) if error.category == "cancelled" => State::Cancelled,
            Err(_) => State::Failed,
        };
        observer.finish(state);
        result
    })
}

pub(super) fn finish_import_outcome(
    ctx: &mut ImportJobContext<'_>,
    source: &domain::DataSource,
    source_conn: &rusqlite::Connection,
    result: Result<String, CommandError>,
) -> Result<String, CommandError> {
    let profiler = ctx.profiler.clone();
    let ready_state = super::execute::ready_state(ctx.content_kind);
    let message = profile::measure_command(profiler.as_ref(), Phase::OutcomePersistence, || {
        super::runtime::persist_outcome(ctx, source, ready_state, result)
    })?;
    let started = Instant::now();
    let message = profile::measure_command(profiler.as_ref(), Phase::PostReady, || {
        super::post_ready::finish(ctx, source, source_conn, message)
    })?;
    ctx.task_stage(transport::dto::TaskStageKeyDto::Analyze, true);
    profile::emit_timed_phase_profile(
        ctx,
        transport::dto::ImportPhaseDto::PostReady,
        transport::dto::ImportPhaseStateDto::Completed,
        99,
        started.elapsed(),
        "Post-ready analysis complete".into(),
    )?;
    ctx.job_repo
        .update_outcome_counts(
            ctx.job_id,
            ctx.counts.warning_count,
            ctx.counts.skipped_count,
            ctx.counts.failed_count,
            ctx.counts.is_partial(),
        )
        .map_err(CommandError::from_service_error)?;
    Ok(message)
}
