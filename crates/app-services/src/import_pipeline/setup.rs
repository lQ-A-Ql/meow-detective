//! Set up source storage and resources without changing their ownership scope.
use super::{context::ImportJobContext, profile::measure_optional};
use crate::connection::command_error_from_db;
use chrono::Utc;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    storage_control_repo::{StorageControlRepo, StorageOperationRecord, StorageOperationState},
};
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
    let operation_id = format!("source-import:{}", ctx.job_id.0);
    let control = StorageControlRepo::new(ctx.conn);
    if control
        .find_operation(&operation_id)
        .map_err(command_error_from_db)?
        .is_none()
    {
        let now = Utc::now().to_rfc3339();
        control
            .insert_operation(&StorageOperationRecord {
                operation_id: operation_id.clone(),
                case_id: ctx.case_id.0.clone(),
                source_id: Some(source.id.0.clone()),
                attempt_id: ctx.job_id.0.clone(),
                operation_kind: "source_import".to_owned(),
                state: StorageOperationState::Planned,
                input_digest: None,
                expected_batch_digest: None,
                created_at: now.clone(),
                updated_at: now,
                last_error_code: None,
            })
            .map_err(command_error_from_db)?;
    }
    control
        .update_operation_state(
            &operation_id,
            StorageOperationState::Running,
            &Utc::now().to_rfc3339(),
            None,
        )
        .map_err(command_error_from_db)?;
    let layout = persistence_fjall::SourceBuildLayout::for_case_source(
        ctx.case_root,
        &source.id.0,
        &ctx.job_id.0,
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    if !layout.published_path().is_dir() {
        persistence_source::detail::bind_generation(
            &conn,
            &format!("source.fjall.build.{}", ctx.job_id.0),
        )
        .map_err(CommandError::from_service_error)?;
    }
    super::begin_partition_projection(ctx.case_root, &source.id)?;
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
