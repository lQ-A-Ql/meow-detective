use super::context::ImportJobContext;
use domain::structure::{SourceRevisionContent, SourceRevisionRef};
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;
use source_kernel::connection::command_error_from_db;
use transport::CommandError;

pub(super) fn bind(
    ctx: &ImportJobContext<'_>,
    source: &domain::DataSource,
    factory: &dyn evidence_core::EvidenceReaderFactory,
    initialize: bool,
) -> Result<u64, CommandError> {
    let plan = ctx.import_config.collection_plan.as_ref();
    let content = plan
        .map(|plan| plan.source_content(&source.source_path, &source.kind))
        .transpose()
        .map_err(CommandError::from_typed_service_error)?;
    let metadata_identity = factory.metadata_identity().map(str::to_owned);
    let content = match content {
        Some(content) => SourceRevisionContent::captured(
            factory.logical_size(),
            metadata_identity,
            content.logical_sha256.clone(),
        ),
        None => SourceRevisionContent::observed(factory.logical_size(), metadata_identity),
    };
    let revision = SourceRevisionRepo::new(ctx.conn)
        .bind(&ctx.case_id.0, &source.id.0, &content, initialize)
        .map_err(command_error_from_db)?;
    Ok(revision.revision)
}

pub(super) fn publish_observations(
    ctx: &ImportJobContext<'_>,
    source: &domain::DataSource,
    receipt: &str,
) -> Result<(), CommandError> {
    let Some(plan) = ctx.import_config.collection_plan.as_ref() else {
        return Ok(());
    };
    let observations = plan
        .native_observations(&source.source_path, &source.kind)
        .map_err(CommandError::from_typed_service_error)?;
    if observations.is_empty() {
        return Ok(());
    }
    let identity = ctx
        .runtime
        .as_ref()
        .ok_or_else(|| CommandError::internal("Source runtime is missing"))?
        .identity();
    let reference = SourceRevisionRef {
        case_id: identity.case_id.clone(),
        source_id: identity.data_source_id.0.clone(),
        revision: identity.revision,
    };
    require_current(ctx.conn, identity)?;
    let source_connection = ctx.source_connection()?;
    let generation = persistence_source::detail::generation_name(source_connection)
        .map_err(command_error_from_db)?;
    let acquisition_id = ctx
        .import_config
        .import_set
        .as_ref()
        .map(|import_set| import_set.import_set_id.as_str())
        .unwrap_or(&source.id.0);
    evidence_access::structure_observation::publish_source_observations(
        ctx.conn,
        source_connection,
        &evidence_access::structure_observation::ObservationPublication {
            case_id: ctx.case_id,
            source: &reference,
            acquisition_id,
            attempt_id: &ctx.job_id.0,
            generation: &generation,
            receipt,
        },
        observations,
    )
    .map_err(|error| CommandError::from_service_error(error.to_string()))?;
    Ok(())
}

pub(super) fn require_current(
    conn: &rusqlite::Connection,
    identity: &evidence_access::source_runtime::SourceIdentity,
) -> Result<(), CommandError> {
    SourceRevisionRepo::new(conn)
        .require_current(&SourceRevisionRef {
            case_id: identity.case_id.clone(),
            source_id: identity.data_source_id.0.clone(),
            revision: identity.revision,
        })
        .map_err(command_error_from_db)
}

pub(super) fn record_collection_verification(
    ctx: &ImportJobContext<'_>,
) -> Result<(), CommandError> {
    let Some(plan) = ctx.import_config.collection_plan.as_ref() else {
        return Ok(());
    };
    let identity = ctx
        .runtime
        .as_ref()
        .ok_or_else(|| CommandError::internal("Collection source runtime missing"))?
        .identity();
    let record = domain::structure::CollectionMemberVerification {
        source: SourceRevisionRef {
            case_id: identity.case_id.clone(),
            source_id: identity.data_source_id.0.clone(),
            revision: identity.revision,
        },
        collection_fingerprint: plan.content_fingerprint(),
    };
    require_current(ctx.conn, identity)?;
    let json = serde_json::to_string(&record).map_err(CommandError::from_service_error)?;
    persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo::new(
        ctx.source_connection()?,
    )
    .write(
        persistence_sqlite::repositories::source_meta_repo::COLLECTION_MEMBER_VERIFICATION_KEY,
        &json,
    )
    .map_err(command_error_from_db)
}
