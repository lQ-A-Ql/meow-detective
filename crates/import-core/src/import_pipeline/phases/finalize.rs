use std::time::{Duration, Instant};
use transport::dto::{ImportPhaseDto, ImportPhaseStateDto};
use transport::CommandError;

use crate::import_pipeline::context::{ImportContentKind, ImportJobContext};
use crate::import_pipeline::profile::elapsed_ms;
use crate::timeline_projection::{self, FILE_PROJECTION_KEY};
use evidence_access::file_service;
use source_kernel::source_db;

mod summary;

pub(crate) fn run_finalize_phase(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    stats: &file_service::EnumerationStats,
    pipeline_message: &str,
) -> Result<String, CommandError> {
    super::super::runtime::validate_current(ctx.conn, ctx.runtime.as_deref())?;
    queue_timeline(ctx, data_source)?;
    ctx.report_job_progress(95, "Finalizing...")?;
    super::super::profile::emit_timed_phase_profile(
        ctx,
        ImportPhaseDto::Finalize,
        ImportPhaseStateDto::Running,
        95,
        Duration::ZERO,
        "Finalizing...".into(),
    )?;
    checkpoint_source_database(ctx, data_source)?;
    Ok(match ctx.content_kind {
        ImportContentKind::Filesystem => {
            summary::build(&ctx.import_config.source_name, stats, pipeline_message)
        }
        ImportContentKind::CephBlueStoreMetadata => format!(
            "Imported {} as Ceph BlueStore metadata-only source. {}",
            ctx.import_config.source_name, pipeline_message
        ),
    })
}

fn queue_timeline(
    ctx: &ImportJobContext<'_>,
    source: &domain::DataSource,
) -> Result<(), CommandError> {
    let (state, detail) = match ctx.content_kind {
        ImportContentKind::Filesystem => {
            let connection = ctx.source_connection()?;
            let identity = format!("pending:{}", source.id.0);
            let required = timeline_projection::plan_required_bytes();
            let available = ctx
                .options
                .projection_policy
                .reservation_bytes()
                .map_err(|error| CommandError::invalid_input(error.to_string()))?;
            let reason = "queued for post-import timeline projection";
            let meta = persistence_sqlite::repositories::timeline_projection_repo::TimelineProjectionRepo::new(connection);
            meta.ensure_meta_table()
                .map_err(CommandError::from_service_error)?;
            meta.mark_deferred(FILE_PROJECTION_KEY, &identity, required, available, reason)
                .map_err(CommandError::from_service_error)?;
            let handle = source_db::open_fjall_source_for_build(connection, &source.id)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            let payload = persistence_source::fjall::TimelineProjectionStatePayload::new(
                persistence_source::fjall::TimelineProjectionState::Deferred,
                identity,
                0,
                required,
                available,
                Some(reason.into()),
            );
            handle
                .timeline_projection_mark_deferred(
                    &source.id.0,
                    &payload,
                    persistence_source::fjall::PersistMode::SyncAll,
                )
                .map_err(|error| CommandError::internal(error.to_string()))?;
            (super::super::capability::QUEUED, reason)
        }
        ImportContentKind::CephBlueStoreMetadata => (
            super::super::capability::NOT_APPLICABLE,
            "metadata-only source has no filesystem timeline",
        ),
    };
    super::super::capability::mark_capability(
        ctx.conn,
        ctx.case_id,
        &source.id,
        "timeline",
        state,
        Some(detail),
        None,
    )
}

pub(crate) fn emit_data_source_ready(
    ctx: &ImportJobContext<'_>,
    source: &domain::DataSource,
) -> Result<(), CommandError> {
    super::super::runtime::validate_current(ctx.conn, ctx.runtime.as_deref())?;
    let summary = file_service::get_data_sources_for_case(ctx.conn, ctx.case_root, ctx.case_id)
        .map_err(CommandError::from_service_error)?
        .into_iter()
        .find(|item| item.id == source.id.0)
        .ok_or_else(|| CommandError::internal("published source summary is missing"))?;
    super::super::runtime::validate_current(ctx.conn, ctx.runtime.as_deref())?;
    super::super::emit::emit_data_source_imported(
        ctx.event_sink(),
        &ctx.case_id.0,
        &summary,
        &ctx.job_id.0,
    );
    Ok(())
}

fn checkpoint_source_database(
    ctx: &ImportJobContext<'_>,
    source: &domain::DataSource,
) -> Result<(), CommandError> {
    let connection = ctx.source_connection()?;
    let started = Instant::now();
    super::super::profile::measure_optional(
        ctx.profiler.as_ref(),
        transport::dto::import_benchmark::ImportBenchmarkPhaseDto::Checkpoint,
        || source_db::checkpoint_source_db(connection),
    )
    .map_err(|error| {
        tracing::error!(data_source_id = %source.id.0, %error, "Source checkpoint failed");
        CommandError::from_service_error(error)
    })?;
    super::super::profile::emit_timed_phase_profile(
        ctx,
        ImportPhaseDto::Checkpoint,
        ImportPhaseStateDto::Completed,
        95,
        started.elapsed(),
        format!(
            "Source checkpoint complete: phase=checkpoint elapsedMs={} rssMb={}",
            elapsed_ms(started.elapsed()),
            source_kernel::runtime_resources::current_rss_mb()
        ),
    )
}
