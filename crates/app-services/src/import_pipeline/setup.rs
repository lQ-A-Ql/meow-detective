//! Set up source storage and resources without changing their ownership scope.
use super::{context::ImportJobContext, profile::measure_optional};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use transport::{dto::import_benchmark::ImportBenchmarkPhaseDto as Phase, CommandError};

pub(super) fn open_source(
    ctx: &ImportJobContext<'_>,
    source: &domain::DataSource,
) -> Result<(rusqlite::Connection, bool), CommandError> {
    let conn = measure_optional(ctx.profiler.as_ref(), Phase::SourceDatabaseOpen, || {
        // Includes connection configuration, migrations and bulk-load recovery.
        crate::source_db::open_source_db(ctx.case_root, &source.id)
            .map_err(CommandError::from_service_error)
    })?;
    let initialize = measure_optional(
        ctx.profiler.as_ref(),
        Phase::SourceMetadataInitialize,
        || {
            DataSourceRepo::new(&conn)
                .upsert_source_local_metadata(ctx.case_id, source)
                .map_err(CommandError::from_service_error)?;
            let pending = super::runtime::source_is_pending(ctx.conn, &source.id)?;
            DataSourceRepo::new(ctx.conn)
                .update_import_state(&source.id, "importing", None)
                .map_err(CommandError::from_service_error)?;
            Ok::<_, CommandError>(pending)
        },
    )?;
    Ok((conn, initialize))
}

pub(super) fn prepare_resources(
    ctx: &mut ImportJobContext<'_>,
    source: &domain::DataSource,
    initialize: bool,
) -> Result<Option<crate::resource_budget::Workspace>, CommandError> {
    let profiler = ctx.profiler.clone();
    measure_optional(profiler.as_ref(), Phase::RuntimePrepare, || {
        super::runtime::prepare_runtime(ctx, source, initialize)
    })?;
    let Some(runtime) = &ctx.runtime else {
        return Ok(None);
    };
    let admitted = measure_optional(profiler.as_ref(), Phase::WorkspaceAdmission, || {
        crate::resource_budget::global_pool()
            .workspace(ctx.options.cancel_token)
            .map_err(CommandError::from_typed_service_error)
    })?;
    runtime.publication_lease().bind_pool(admitted.pool());
    Ok(Some(admitted))
}
