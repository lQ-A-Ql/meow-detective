use std::time::{Duration, Instant};

use transport::dto::{
    ImportPhaseDto, ImportPhaseStateDto, PartialResultDto, PartialResultKindDto, ResultFreshnessDto,
};
use transport::CommandError;

use crate::timeline_projection::{self, TimelineProjectionError, FILE_PROJECTION_KEY};
use evidence_access::file_service;
use source_kernel::source_db;

use crate::import_pipeline::context::ImportJobContext;
use crate::import_pipeline::emit;
use crate::import_pipeline::profile::elapsed_ms;

mod budget;

/// Three-state materialization result: completed with an event count, or
/// capacity-deferred (import keeps running as partial), or propagated as a
/// fatal error by the caller.
enum TimelineOutcome {
    Completed(u64),
    Deferred {
        required_bytes: u64,
        available_bytes: u64,
        identity: String,
    },
}

pub(crate) fn run_finalize_phase(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    stats: &file_service::EnumerationStats,
    pipeline_message: &str,
) -> Result<String, CommandError> {
    super::super::runtime::validate_current(ctx.runtime.as_deref())?;
    let detached = matches!(
        ctx.post_import_mode,
        crate::import_pipeline::options::PostImportMode::Detached
    );
    let timeline_outcome = if detached {
        // Catalog-only path: timeline materialization runs in the post-import
        // scheduler. Mark the capability instead of materializing here, and
        // never emit synthetic timeline/search-index completion events.
        mark_detached_timeline_capability(ctx, data_source)?;
        None
    } else if ctx.content_kind == crate::import_pipeline::context::ImportContentKind::Filesystem {
        Some(materialize_timeline(ctx, data_source)?)
    } else {
        None
    };
    if let Some(TimelineOutcome::Deferred {
        required_bytes,
        available_bytes,
        identity,
    }) = &timeline_outcome
    {
        defer_timeline(
            ctx,
            data_source,
            *required_bytes,
            *available_bytes,
            identity,
        )?;
    }
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
    if let Some(TimelineOutcome::Completed(timeline_count)) = &timeline_outcome {
        emit_projection_ready_events(ctx, *timeline_count);
    }
    Ok(match ctx.content_kind {
        crate::import_pipeline::context::ImportContentKind::Filesystem => {
            build_summary_message(&ctx.import_config.source_name, stats, pipeline_message)
        }
        crate::import_pipeline::context::ImportContentKind::CephBlueStoreMetadata => format!(
            "Imported {} as Ceph BlueStore metadata-only source. {}",
            ctx.import_config.source_name, pipeline_message
        ),
    })
}

/// Detached catalog-only finalize: queue timeline for the post-import
/// scheduler on filesystem sources; metadata-only sources cannot carry a
/// timeline projection. The graph capability is derived from the timeline
/// marker and is not written separately here.
fn mark_detached_timeline_capability(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<(), CommandError> {
    if ctx.content_kind == crate::import_pipeline::context::ImportContentKind::Filesystem {
        write_deferred_marker(
            ctx.source_connection()?,
            data_source,
            &format!("pending:{}", data_source.id.0),
            timeline_projection::plan_required_bytes(),
            ctx.options
                .projection_policy
                .reservation_bytes()
                .map_err(|error| CommandError::invalid_input(error.to_string()))?,
            "queued for post-import timeline projection",
        )?;
    }
    let (state, detail) =
        if ctx.content_kind == crate::import_pipeline::context::ImportContentKind::Filesystem {
            (
                crate::import_pipeline::capability::QUEUED,
                Some("queued for post-import timeline projection"),
            )
        } else {
            (
                crate::import_pipeline::capability::NOT_APPLICABLE,
                Some("metadata-only source has no filesystem timeline"),
            )
        };
    crate::import_pipeline::capability::mark_capability(
        ctx.conn,
        ctx.case_id,
        &data_source.id,
        "timeline",
        state,
        detail,
        None,
    )
}

fn emit_projection_ready_events(ctx: &ImportJobContext<'_>, timeline_count: u64) {
    crate::import_pipeline::emit::emit_timeline_updated(ctx.event_sink(), timeline_count);
    crate::import_pipeline::emit::emit_search_index_progress(
        ctx.event_sink(),
        100,
        "Post-import indexing completed",
    );
}

fn materialize_timeline(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<TimelineOutcome, CommandError> {
    let pool = source_kernel::resource_budget::pool_for(
        ctx.runtime.as_ref().map(|r| r.publication_lease()).as_ref(),
    );
    let planned = timeline_projection::plan_required_bytes();
    let projection = budget::run(
        &pool,
        ctx.options.projection_policy,
        planned,
        ctx.options.cancel_token.as_ref(),
        || {
            let source_conn = ctx.source_connection().map_err(other_command_error)?;
            ctx.report_job_progress(94, "Materializing timeline...")
                .map_err(other_command_error)?;
            super::super::profile::measure_optional(
                ctx.profiler.as_ref(),
                transport::dto::import_benchmark::ImportBenchmarkPhaseDto::Timeline,
                || {
                    timeline_projection::materialize_file_activity_with_progress(
                        source_conn,
                        ctx.import_config.platform,
                        ctx.options.cancel_token.as_ref(),
                        ctx.options.projection_policy,
                        Some(&|work, done, total| {
                            if let Some(sink) = ctx.event_sink() {
                                sink.timeline_work(work, done, total);
                            }
                        }),
                    )
                },
            )
        },
    );
    match projection {
        Ok(projection) => {
            record_timeline_subphases(ctx, &projection);
            super::super::profile::emit_timed_phase_profile(
                ctx,
                ImportPhaseDto::Timeline,
                if projection.graph_complete {
                    ImportPhaseStateDto::Completed
                } else {
                    ImportPhaseStateDto::Partial
                },
                94,
                Duration::from_millis(projection.elapsed_ms.min(u64::MAX as u128) as u64),
                format!(
                    "Timeline materialization complete: phase=timeline elapsedMs={} eventMs={} graphMs={} rows={} warnings={} rssMb={}",
                    projection.elapsed_ms,
                    projection.events_elapsed_ms,
                    projection.graph_elapsed_ms,
                    projection.inserted_count,
                    projection.warnings.len(),
                    source_kernel::runtime_resources::current_rss_mb()
                ),
            )?;
            ctx.counts.add_warnings(projection.warnings.len());
            for warning in &projection.warnings {
                tracing::warn!(
                    data_source_id = %data_source.id.0,
                    warning,
                    "Timeline graph materialization warning"
                );
            }
            Ok(TimelineOutcome::Completed(projection.inserted_count))
        }
        Err(TimelineProjectionError::WorkspaceLimit {
            required_bytes,
            available_bytes,
            identity,
            ..
        }) => Ok(TimelineOutcome::Deferred {
            required_bytes,
            available_bytes,
            identity,
        }),
        Err(error) => Err(CommandError::from_typed_service_error(error)),
    }
}

fn other_command_error(error: CommandError) -> TimelineProjectionError {
    TimelineProjectionError::Other(error.to_string())
}

/// Capacity-deferred side effects, in contract order: SQLite meta `deferred` →
/// fjall `deferred` marker → warning count → Partial phase=timeline progress →
/// `TimelineEvents` partial result with `Deferred` freshness. Never emits
/// `timeline_updated(0)`, never writes the graph done marker, and never fails
/// the job.
fn defer_timeline(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    required_bytes: u64,
    available_bytes: u64,
    identity: &str,
) -> Result<(), CommandError> {
    let source_conn = ctx.source_connection()?;
    let source_id = persistence_source::detail::source_id(source_conn)
        .map_err(|error| CommandError::internal(error.to_string()))?
        .ok_or_else(|| CommandError::internal("source identity is missing"))?;
    let reason =
        format!("estimated {required_bytes} bytes exceeds configured {available_bytes} bytes");
    write_deferred_marker(
        source_conn,
        data_source,
        identity,
        required_bytes,
        available_bytes,
        &reason,
    )?;
    ctx.counts.add_warnings(1);
    let detail = format!(
        "Timeline projection deferred: status=deferred requiredBytes={required_bytes} availableBytes={available_bytes} reason={reason}"
    );
    super::super::profile::emit_timed_phase_profile(
        ctx,
        ImportPhaseDto::Timeline,
        ImportPhaseStateDto::Partial,
        94,
        Duration::ZERO,
        detail,
    )?;
    let partial = PartialResultDto {
        kind: PartialResultKindDto::TimelineEvents,
        scope_id: source_id.clone(),
        ready_count: 0,
        total_estimate: None,
        query_key: format!("timeline:events:{source_id}"),
        freshness: ResultFreshnessDto::Deferred,
    };
    emit::emit_import_partial_result(ctx.event_sink(), &partial);
    Ok(())
}

/// Writes the SQLite `timeline_projection_meta` deferred row and the fjall
/// `timeline_projection_deferred` marker. Split out for unit testing without a
/// full import job context.
pub(crate) fn write_deferred_marker(
    source_conn: &rusqlite::Connection,
    data_source: &domain::DataSource,
    identity: &str,
    required_bytes: u64,
    available_bytes: u64,
    reason: &str,
) -> Result<(), CommandError> {
    let source_id = persistence_source::detail::source_id(source_conn)
        .map_err(|error| CommandError::internal(error.to_string()))?
        .ok_or_else(|| CommandError::internal("source identity is missing"))?;
    let meta =
        persistence_sqlite::repositories::timeline_projection_repo::TimelineProjectionRepo::new(
            source_conn,
        );
    meta.ensure_meta_table()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    meta.mark_deferred(
        FILE_PROJECTION_KEY,
        identity,
        required_bytes,
        available_bytes,
        reason,
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    let handle = source_db::open_fjall_source_for_build(source_conn, &data_source.id)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let payload = persistence_source::fjall::TimelineProjectionStatePayload::new(
        persistence_source::fjall::TimelineProjectionState::Deferred,
        identity.to_owned(),
        0,
        required_bytes,
        available_bytes,
        Some(reason.to_owned()),
    );
    handle
        .timeline_projection_mark_deferred(
            &source_id,
            &payload,
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(())
}

fn record_timeline_subphases(
    ctx: &ImportJobContext<'_>,
    projection: &timeline_projection::TimelineProjectionStats,
) {
    use super::super::profile::Counters;
    use transport::dto::import_benchmark::{
        ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
    };
    let Some(profiler) = ctx.profiler.as_ref() else {
        return;
    };
    for (phase, elapsed, rows) in [
        (
            Phase::TimelineEvents,
            projection.events_elapsed_ms,
            projection.inserted_count,
        ),
        (Phase::TimelineGraph, projection.graph_elapsed_ms, 0),
    ] {
        let state = if projection.already_projected {
            State::NotApplicable
        } else if phase == Phase::TimelineGraph && !projection.graph_complete {
            State::Failed
        } else {
            State::Completed
        };
        let duration = (!projection.already_projected)
            .then(|| std::time::Duration::from_millis(u64::try_from(elapsed).unwrap_or(u64::MAX)));
        profiler.record(
            None,
            phase,
            duration,
            state,
            Counters {
                staged: rows,
                sampled_wal_bytes: if phase == Phase::TimelineGraph && !projection.already_projected
                {
                    ctx.source_conn
                        .and_then(super::super::profile::sample_wal_size)
                } else {
                    None
                },
                ..Counters::default()
            },
        );
    }
    record_timeline_write_timings(profiler, projection);
}

fn record_timeline_write_timings(
    profiler: &crate::import_pipeline::ImportProfiler,
    projection: &timeline_projection::TimelineProjectionStats,
) {
    use super::super::profile::Counters;
    use transport::dto::import_benchmark::{
        ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
    };
    let timings = &projection.timings;
    for (phase, elapsed, graph) in [
        (Phase::TimelineEventClear, timings.event_clear, false),
        (Phase::TimelineEventCursor, timings.event_cursor, false),
        (Phase::TimelineEventInsert, timings.event_insert, false),
        (Phase::TimelineEventIndexes, timings.event_indexes, false),
        (Phase::TimelineEventCommit, timings.event_commit, false),
        (Phase::TimelineGraphPage, timings.graph_page, true),
        (
            Phase::TimelineGraphNodePrepare,
            timings.graph_node_prepare,
            true,
        ),
        (
            Phase::TimelineGraphEdgePrepare,
            timings.graph_edge_prepare,
            true,
        ),
        (
            Phase::TimelineGraphPagePrepare,
            timings.graph_page_prepare,
            true,
        ),
        (Phase::TimelineGraphNodes, timings.graph_nodes, true),
        (Phase::TimelineGraphEdges, timings.graph_edges, true),
        (Phase::TimelineGraphCommit, timings.graph_commit, true),
    ] {
        let state = if projection.already_projected {
            State::NotApplicable
        } else if graph && !projection.graph_complete {
            State::Failed
        } else {
            State::Completed
        };
        profiler.record(
            None,
            phase,
            (!projection.already_projected).then_some(elapsed),
            state,
            Counters::default(),
        );
    }
}

pub(crate) fn emit_data_source_ready(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<(), CommandError> {
    super::super::runtime::validate_current(ctx.runtime.as_deref())?;
    let summary = file_service::get_data_sources_for_case(ctx.conn, ctx.case_root, ctx.case_id)
        .map_err(CommandError::from_service_error)?
        .into_iter()
        .find(|source| source.id == data_source.id.0);
    if let Some(summary) = summary {
        super::super::runtime::validate_current(ctx.runtime.as_deref())?;
        crate::import_pipeline::emit::emit_data_source_imported(
            ctx.event_sink(),
            &ctx.case_id.0,
            &summary,
            &ctx.job_id.0,
        );
    } else {
        tracing::warn!(
            "Imported data source {} was not found in summary list for event emission",
            data_source.id.0
        );
    }
    Ok(())
}

fn checkpoint_source_database(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<(), CommandError> {
    let Some(source_conn) = ctx.source_conn else {
        return Ok(());
    };
    let started = Instant::now();
    super::super::profile::measure_optional(
        ctx.profiler.as_ref(),
        transport::dto::import_benchmark::ImportBenchmarkPhaseDto::Checkpoint,
        || source_db::checkpoint_source_db(source_conn),
    )
    .map_err(|error| {
        tracing::error!(
            data_source_id = %data_source.id.0,
            error = %error,
            "Source DB WAL checkpoint failed during import finalization"
        );
        CommandError::from_service_error(error)
    })?;
    super::super::profile::emit_timed_phase_profile(
        ctx,
        transport::dto::ImportPhaseDto::Checkpoint,
        transport::dto::ImportPhaseStateDto::Completed,
        95,
        started.elapsed(),
        format!(
            "Source checkpoint complete: phase=checkpoint elapsedMs={} rssMb={}",
            elapsed_ms(started.elapsed()),
            source_kernel::runtime_resources::current_rss_mb()
        ),
    )?;
    Ok(())
}

fn build_summary_message(
    source_name: &str,
    stats: &file_service::EnumerationStats,
    pipeline_message: &str,
) -> String {
    let mut message = format!(
        "Imported {source_name}: {} files, {} dirs",
        stats.file_count, stats.dir_count
    );
    if !pipeline_message.is_empty() {
        message.push_str(". ");
        message.push_str(pipeline_message);
    }
    message
}
