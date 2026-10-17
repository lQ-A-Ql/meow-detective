//! Publish the validated catalog indexes and graph before analysis starts.
use crate::import_pipeline::profile::emit_phase_profile;
use crate::{file_service, import_pipeline::context::ImportJobContext};
use transport::CommandError;

pub(super) fn finish_catalog(
    ctx: &mut ImportJobContext<'_>,
    source: &domain::DataSource,
    stats: &mut file_service::EnumerationStats,
) -> Result<(), CommandError> {
    super::super::runtime::validate_current(ctx.runtime.as_deref())?;
    super::super::runtime::finish_indexes(ctx)?;
    populate_file_graph(ctx, source, stats);
    ctx.counts.add_warnings(stats.warnings.len());
    report_catalog_ready(ctx, source, stats);
    Ok(())
}

fn populate_file_graph(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    stats: &mut file_service::EnumerationStats,
) {
    let Some(source_conn) = ctx.source_conn else {
        return;
    };
    if let Err(error) = super::super::profile::measure_optional(
        ctx.profiler.as_ref(),
        transport::dto::import_benchmark::ImportBenchmarkPhaseDto::Graph,
        || file_service::populate_file_graph_for_data_source(source_conn, &data_source.id),
    ) {
        let warning = format!("Graph population warning: {error}");
        tracing::warn!(%warning, "Failed to populate file graph after enumeration");
        stats.warnings.push(warning);
    }
}

fn report_catalog_ready(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    stats: &file_service::EnumerationStats,
) {
    emit_phase_profile(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        70,
        format!(
            "File catalog ready: phase=enum-merge rows={} files={} dirs={} warnings={} rssMb={}",
            stats.file_count + stats.dir_count,
            stats.file_count,
            stats.dir_count,
            stats.warnings.len(),
            crate::runtime_resources::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );
}
