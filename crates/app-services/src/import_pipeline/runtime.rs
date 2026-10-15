use super::context::ImportJobContext;
use transport::CommandError;

pub(super) fn prepare_runtime(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
) -> Result<(), CommandError> {
    if matches!(
        ctx.import_config.kind,
        domain::DataSourceKind::Raw
            | domain::DataSourceKind::E01
            | domain::DataSourceKind::LocalDisk
    ) {
        let factory = crate::source_runtime::open_factory(
            &ctx.import_config.source_path,
            &ctx.import_config.kind,
        )
        .map_err(CommandError::from_typed_service_error)?;
        ctx.runtime = Some(std::sync::Arc::new(
            crate::source_runtime::SourceRuntime::new(
                crate::source_runtime::SourceIdentity {
                    case_id: ctx.case_id.0.clone(),
                    data_source_id: ds.id.clone(),
                    revision: 0,
                    source_kind: ctx.import_config.kind.clone(),
                    path: ctx.import_config.source_path.clone(),
                },
                factory,
            ),
        ));
    }

    Ok(())
}

pub(super) fn finish_indexes(ctx: &ImportJobContext<'_>) -> Result<(), CommandError> {
    super::profile::measure_optional(
        ctx.profiler.as_ref(),
        transport::dto::import_benchmark::ImportBenchmarkPhaseDto::SourceIndexBuild,
        || {
            persistence_sqlite::repositories::source_bulk_load_repo::SourceBulkLoadRepo::finish(
                ctx.source_connection()?,
            )
            .map_err(CommandError::from_typed_service_error)
        },
    )?;
    Ok(())
}
