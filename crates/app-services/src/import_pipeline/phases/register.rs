use persistence_sqlite::repositories::linux_import_set_repo::LinuxImportSetRepo;
use transport::CommandError;

use crate::datasource_service;
use crate::import_pipeline::context::{ImportJobContext, PhaseTelemetry};
use crate::import_pipeline::profile::emit_phase_profile;

pub(crate) fn run_attach_phase(
    ctx: &mut ImportJobContext<'_>,
) -> Result<domain::DataSource, CommandError> {
    let source_name = ctx.import_config.source_name.clone();
    let path = ctx.import_config.source_path.clone();
    let kind = ctx.import_config.kind.clone();

    ctx.report_job_progress(10, &format!("Attaching data source {source_name}"))?;

    let telemetry = PhaseTelemetry::new();
    let data_source = datasource_service::attach_data_source_with_storage(
        ctx.conn,
        ctx.case_id,
        &source_name,
        &path,
        kind,
        ctx.import_config.platform,
        ctx.import_config.profile.clone(),
    )
    .map_err(CommandError::from_service_error)?;

    persist_import_set_membership(ctx, &data_source)?;
    emit_phase_profile(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        12,
        format!(
            "Attach complete: phase=attach elapsedMs={} rssMb={}",
            telemetry.elapsed_ms(),
            crate::runtime_resources::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );
    Ok(data_source)
}

fn persist_import_set_membership(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<(), CommandError> {
    let Some(import_set) = &ctx.import_config.import_set else {
        return Ok(());
    };
    LinuxImportSetRepo::new(ctx.conn)
        .bind_member_source(
            &import_set.import_set_id,
            import_set.member_index,
            &data_source.id.0,
        )
        .map_err(CommandError::from_service_error)
}
