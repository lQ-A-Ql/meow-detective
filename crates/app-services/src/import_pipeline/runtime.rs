use super::context::ImportJobContext;
use transport::CommandError;

pub(super) fn source_is_pending(
    conn: &rusqlite::Connection,
    source: &domain::DataSourceId,
) -> Result<bool, CommandError> {
    Ok(
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
            .find_storage(source)
            .map_err(CommandError::from_typed_service_error)?
            .is_some_and(|storage| storage.import_state == "pending"),
    )
}

pub(super) fn validate_current(
    runtime: Option<&crate::source_runtime::SourceRuntime>,
) -> Result<(), CommandError> {
    if let Some(runtime) = runtime {
        runtime
            .publication_lease()
            .validate()
            .map_err(CommandError::from_typed_service_error)?;
    }
    Ok(())
}

pub(super) fn persist_outcome(
    ctx: &ImportJobContext<'_>,
    ds: &domain::DataSource,
    ready_state: &str,
    result: Result<String, CommandError>,
) -> Result<String, CommandError> {
    match result {
        Ok(message) => publish_outcome(ctx, ds, ready_state, message),
        Err(error) => {
            record_failure(ctx, ds, &error);
            Err(error)
        }
    }
}

fn record_failure(ctx: &ImportJobContext<'_>, ds: &domain::DataSource, error: &CommandError) {
    let lease = ctx.runtime.as_ref().map(|r| r.publication_lease());
    if lease
        .as_ref()
        .is_some_and(|lease| !lease.owns_failure_state())
    {
        return;
    }
    let _fence = lease
        .as_ref()
        .map(|lease| lease.failure_fence_connection(ctx.conn));
    super::execute::persist_failed_import(ctx.conn, &ds.id, error);
}

fn publish_outcome(
    ctx: &ImportJobContext<'_>,
    ds: &domain::DataSource,
    ready_state: &str,
    message: String,
) -> Result<String, CommandError> {
    match validate_current(ctx.runtime.as_deref()) {
        Ok(()) => {
            let lease = ctx.runtime.as_ref().map(|r| r.publication_lease());
            let result = {
                let _fence = lease.as_ref().map(|lease| lease.fence_connection(ctx.conn));
                super::execute::persist_import_outcome(ctx.conn, &ds.id, ready_state, Ok(message))
            };
            if let Err(error) = &result {
                record_failure(ctx, ds, error);
            }
            result
        }
        Err(error) => {
            record_failure(ctx, ds, &error);
            Err(error)
        }
    }
}

pub(super) fn prepare_runtime(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
    initialize_view: bool,
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
        crate::source_runtime::resume::bind_source_view(
            ctx.source_connection()?,
            &ds.id,
            factory.as_ref(),
            ctx.case_root,
            initialize_view,
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
