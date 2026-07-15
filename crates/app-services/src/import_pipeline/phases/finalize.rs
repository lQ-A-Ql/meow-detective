use std::time::Instant;

use transport::CommandError;

use crate::{file_service, import_analysis, source_db, step_recorder};

use crate::import_pipeline::context::ImportJobContext;
use crate::import_pipeline::profile::{elapsed_ms, emit_phase_profile};

pub(crate) fn run_finalize_phase(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    stats: &file_service::EnumerationStats,
    pipeline_message: &str,
    import_started: Instant,
) -> Result<String, CommandError> {
    if ctx.content_kind == crate::import_pipeline::context::ImportContentKind::Filesystem {
        emit_projection_ready_events(ctx, stats);
    }
    ctx.report_job_progress(95, "Finalizing...")?;
    emit_phase_profile(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        99,
        format!(
            "Import profile complete: phase=total elapsedMs={} rssMb={}",
            elapsed_ms(import_started.elapsed()),
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );
    checkpoint_source_database(ctx, data_source)?;
    record_import_step(ctx, stats, import_started);
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

fn emit_projection_ready_events(
    ctx: &ImportJobContext<'_>,
    stats: &file_service::EnumerationStats,
) {
    crate::import_pipeline::emit::emit_timeline_updated(
        ctx.event_sink(),
        stats.file_count + stats.dir_count,
    );
    crate::import_pipeline::emit::emit_search_index_progress(
        ctx.event_sink(),
        100,
        "Post-import indexing completed",
    );
}

pub(crate) fn emit_data_source_ready(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<(), CommandError> {
    let summary = file_service::get_data_sources_for_case(ctx.conn, ctx.case_root, ctx.case_id)
        .map_err(CommandError::from_service_error)?
        .into_iter()
        .find(|source| source.id == data_source.id.0);
    if let Some(summary) = summary {
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
    source_db::checkpoint_source_db(source_conn).map_err(|error| {
        tracing::error!(
            data_source_id = %data_source.id.0,
            error = %error,
            "Source DB WAL checkpoint failed during import finalization"
        );
        CommandError::from_service_error(error)
    })
}

fn record_import_step(
    ctx: &ImportJobContext<'_>,
    stats: &file_service::EnumerationStats,
    import_started: Instant,
) {
    let params = serde_json::json!({
        "sourcePath": ctx.source_path,
        "sourceName": ctx.import_config.source_name,
        "kind": format!("{:?}", ctx.import_config.kind),
        "contentKind": match ctx.content_kind {
            crate::import_pipeline::context::ImportContentKind::Filesystem => "filesystem",
            crate::import_pipeline::context::ImportContentKind::CephBlueStoreMetadata => {
                "cephBlueStoreMetadata"
            }
        },
        "filesEnumerated": stats.file_count,
        "dirsEnumerated": stats.dir_count,
    })
    .to_string();
    let _ = step_recorder::record_step(
        ctx.conn,
        &ctx.case_id.0,
        "import",
        &params,
        import_started.elapsed().as_millis() as u32,
        true,
        None,
    );
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
