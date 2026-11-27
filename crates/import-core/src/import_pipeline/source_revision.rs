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
    let observations = plan
        .map(|plan| plan.native_observations(&source.source_path, &source.kind))
        .transpose()
        .map_err(CommandError::from_typed_service_error)?
        .unwrap_or_default();
    let revision = SourceRevisionRepo::new(ctx.conn)
        .bind(
            &ctx.case_id.0,
            &source.id.0,
            &SourceRevisionContent {
                logical_size: factory.logical_size(),
                metadata_identity: factory.metadata_identity().map(str::to_owned),
                logical_sha256: content.map(|content| content.logical_sha256.clone()),
            },
            observations,
            initialize,
        )
        .map_err(command_error_from_db)?;
    Ok(revision.revision)
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
