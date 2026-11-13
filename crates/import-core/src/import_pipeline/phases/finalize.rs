use std::time::Instant;

use transport::CommandError;

use crate::timeline_projection;
use evidence_access::file_service;
use source_kernel::source_db;

use crate::import_pipeline::context::ImportJobContext;
use crate::import_pipeline::profile::elapsed_ms;

mod budget;

pub(crate) fn run_finalize_phase(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    stats: &file_service::EnumerationStats,
    pipeline_message: &str,
) -> Result<String, CommandError> {
    super::super::runtime::validate_current(ctx.runtime.as_deref())?;
    let timeline_count =
        if ctx.content_kind == crate::import_pipeline::context::ImportContentKind::Filesystem {
            Some(materialize_timeline(ctx, data_source)?)
        } else {
            None
        };
    ctx.report_job_progress(95, "Finalizing...")?;
    checkpoint_source_database(ctx, data_source)?;
    if let Some(timeline_count) = timeline_count {
        emit_projection_ready_events(ctx, timeline_count);
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
) -> Result<u64, CommandError> {
    let pool = source_kernel::resource_budget::pool_for(
        ctx.runtime.as_ref().map(|r| r.publication_lease()).as_ref(),
    );
    let projection = budget::run(
        &pool,
        ctx.options.projection_policy,
        ctx.options.cancel_token,
        || {
            let source_conn = ctx.source_connection()?;
            ctx.report_job_progress(94, "Materializing timeline...")?;
            super::super::profile::measure_optional(
                ctx.profiler.as_ref(),
                transport::dto::import_benchmark::ImportBenchmarkPhaseDto::Timeline,
                || {
                    timeline_projection::materialize_file_activity_with_policy(
                        source_conn,
                        ctx.import_config.platform,
                        ctx.options.cancel_token.as_ref(),
                        ctx.options.projection_policy,
                    )
                },
            )
            .map_err(CommandError::from_typed_service_error)
        },
    )?;
    record_timeline_subphases(ctx, &projection);
    super::super::profile::emit_timed_phase_profile(
        ctx,
        transport::dto::ImportPhaseDto::Timeline,
        if projection.graph_complete { transport::dto::ImportPhaseStateDto::Completed }
        else { transport::dto::ImportPhaseStateDto::Partial },
        94,
        std::time::Duration::from_millis(projection.elapsed_ms.min(u64::MAX as u128) as u64),
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
    Ok(projection.inserted_count)
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
