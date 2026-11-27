use super::context::ImportJobContext;
use chrono::Utc;
use persistence_sqlite::repositories::storage_control_repo::{
    StorageControlRepo, StorageOperationState,
};
use source_kernel::connection::command_error_from_db;
use transport::CommandError;

pub(super) fn source_is_pending(
    conn: &rusqlite::Connection,
    source: &domain::DataSourceId,
) -> Result<bool, CommandError> {
    Ok(
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
            .find_storage(source)
            .map_err(command_error_from_db)?
            .is_some_and(|storage| storage.import_state == "pending"),
    )
}

pub(super) fn validate_current(
    runtime: Option<&evidence_access::source_runtime::SourceRuntime>,
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
            let state = if error.category == "cancelled" {
                StorageOperationState::Cancelled
            } else {
                StorageOperationState::Failed
            };
            mark_storage_operation(ctx, state, Some(&error.code))?;
            record_failure(ctx, ds, &error);
            Err(error)
        }
    }
}

fn mark_storage_operation(
    ctx: &ImportJobContext<'_>,
    state: StorageOperationState,
    error_code: Option<&str>,
) -> Result<(), CommandError> {
    let operation_id = format!("source-import:{}", ctx.job_id.0);
    StorageControlRepo::new(ctx.conn)
        .update_operation_state(&operation_id, state, &Utc::now().to_rfc3339(), error_code)
        .map_err(command_error_from_db)?;
    Ok(())
}

pub(super) fn record_failure(
    ctx: &ImportJobContext<'_>,
    ds: &domain::DataSource,
    error: &CommandError,
) {
    let operation_id = format!("source-import:{}", ctx.job_id.0);
    let operation_state = StorageControlRepo::new(ctx.conn)
        .find_operation(&operation_id)
        .ok()
        .flatten()
        .map(|operation| operation.state);
    if operation_state != Some(StorageOperationState::Cancelled) {
        if let Err(storage_error) =
            mark_storage_operation(ctx, StorageOperationState::Failed, Some(&error.to_string()))
        {
            tracing::warn!(error = %storage_error, "Failed to mark storage operation as failed");
        }
    }
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
    let state = if error.category == "cancelled" {
        "cancelled"
    } else {
        "failed"
    };
    for capability in ["timeline", "artifacts"] {
        if let Err(update_error) = super::capability::mark_capability(
            ctx.conn,
            ctx.case_id,
            &ds.id,
            capability,
            state,
            Some(&error.message),
            Some(&error.code),
        ) {
            tracing::error!(%update_error, capability, "Failed to close import capability state");
        }
    }
}

fn publish_outcome(
    ctx: &ImportJobContext<'_>,
    ds: &domain::DataSource,
    ready_state: &str,
    message: String,
) -> Result<String, CommandError> {
    let validation =
        ctx.import_config
            .collection_plan
            .as_ref()
            .map_or(Ok(()), |plan| {
                let member =
                    ctx.import_config.import_set.as_ref().ok_or_else(|| {
                        CommandError::internal("Collection member identity missing")
                    })?;
                plan.verify_member_content(
                    member.member_index as usize,
                    &ctx.import_config.source_path,
                    &ctx.import_config.kind,
                    ctx.options.cancel_token,
                )
                .map_err(CommandError::from_typed_service_error)
            })
            .and_then(|()| validate_current(ctx.runtime.as_deref()));
    match validation {
        Ok(()) => {
            let lease = ctx.runtime.as_ref().map(|r| r.publication_lease());
            let result = {
                let _fence = lease.as_ref().map(|lease| lease.fence_connection(ctx.conn));
                super::execute::persist_import_outcome(ctx.conn, &ds.id, ready_state, Ok(message))
            };
            if result.is_ok() {
                mark_storage_operation(ctx, StorageOperationState::Published, None)?;
            }
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
            | domain::DataSourceKind::NestedRange
    ) {
        let factory: evidence_core::SharedReaderFactory =
            if ds.kind == domain::DataSourceKind::NestedRange {
                evidence_access::source_runtime::nested_range::NestedRangeFactory::for_source(
                    ctx.conn,
                    ctx.case_root,
                    ctx.case_id,
                    &ds.id,
                )
                .map(|factory| std::sync::Arc::new(factory) as evidence_core::SharedReaderFactory)
            } else if let Some(plan) = &ctx.import_config.collection_plan {
                plan.verified_factory(
                    &ctx.import_config.source_path,
                    &ctx.import_config.kind,
                    ctx.options.cancel_token,
                )
                .map_err(std::io::Error::other)
            } else {
                evidence_access::source_runtime::factory::open_factory_for_case(
                    &ctx.import_config.source_path,
                    &ctx.import_config.kind,
                    ctx.options.cancel_token,
                    &ctx.case_id.0,
                )
            }
            .map_err(CommandError::from_typed_service_error)?;
        if ds.kind == domain::DataSourceKind::NestedRange {
            factory
                .validate_snapshot()
                .map_err(CommandError::from_typed_service_error)?;
        }
        crate::resume::bind_source_view(
            ctx.source_connection()?,
            &ds.id,
            factory.as_ref(),
            ctx.case_root,
            initialize_view,
        )
        .map_err(CommandError::from_typed_service_error)?;
        crate::resume::bind_collection_view(
            ctx.source_connection()?,
            ctx.import_config
                .collection_plan
                .as_ref()
                .map(|plan| plan.content_fingerprint())
                .as_deref(),
            initialize_view,
        )
        .map_err(CommandError::from_typed_service_error)?;
        ctx.runtime = Some(std::sync::Arc::new(
            evidence_access::source_runtime::SourceRuntime::new(
                evidence_access::source_runtime::SourceIdentity {
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
            let source = persistence_source::detail::open_store(ctx.source_connection()?)
                .map_err(command_error_from_db)?
                .ok_or_else(|| CommandError::internal("source generation is missing"))?;
            source
                .database()
                .persist(persistence_source::fjall::PersistMode::SyncAll)
                .map_err(|error| CommandError::internal(error.to_string()))
        },
    )?;
    Ok(())
}
