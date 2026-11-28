use super::context::ImportJobContext;
use domain::structure::{
    AcquisitionId, ByteRange, ReadIntent, ReadPriority, ReadVerification, SourceRevisionContent,
    SourceRevisionRef, StructureAttemptRef,
};
use evidence_access::structure_access::{
    open_verified_view, plan_target_access, AccessTarget, TargetAccessRequest,
};
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;
use source_kernel::connection::command_error_from_db;
use transport::CommandError;

pub(super) fn bind_catalog_source(
    ctx: &ImportJobContext<'_>,
    source: &domain::DataSource,
    logical_size: u64,
    conn: &rusqlite::Connection,
) -> Result<(), CommandError> {
    if !matches!(
        source.kind,
        domain::DataSourceKind::LogicalDirectory | domain::DataSourceKind::LogicalArchive
    ) {
        return Ok(());
    }
    let store = persistence_source::detail::require_source_store(ctx.source_connection()?)
        .map_err(command_error_from_db)?;
    let manifest = persistence_source::fjall::read_source_generation_manifest(&store, &source.id.0)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let acquisition = ctx
        .import_config
        .import_set
        .as_ref()
        .map(|set| set.import_set_id.clone())
        .unwrap_or_else(|| source.id.0.clone());
    SourceRevisionRepo::new(conn)
        .bind(
            &ctx.case_id.0,
            &source.id.0,
            &SourceRevisionContent::observed(
                AcquisitionId(acquisition),
                logical_size,
                Some(format!("source-catalog:{}", manifest.immutable_data_digest)),
            ),
            true,
        )
        .map_err(command_error_from_db)?;
    Ok(())
}

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
    let acquisition_id = AcquisitionId(
        ctx.import_config
            .import_set
            .as_ref()
            .map(|import_set| import_set.import_set_id.clone())
            .unwrap_or_else(|| source.id.0.clone()),
    );
    let content = match content {
        Some(content) => SourceRevisionContent::captured(
            acquisition_id.clone(),
            factory.logical_size(),
            metadata_identity,
            content.logical_sha256.clone(),
        ),
        None => SourceRevisionContent::observed(
            acquisition_id,
            factory.logical_size(),
            metadata_identity,
        ),
    };
    let revision = SourceRevisionRepo::new(ctx.conn)
        .bind(&ctx.case_id.0, &source.id.0, &content, initialize)
        .map_err(command_error_from_db)?;
    if let Some(plan) = plan {
        evidence_access::source_runtime::content::persist_registered_content(
            ctx.source_connection()?,
            &revision,
            plan.source_content(&source.source_path, &source.kind)
                .map_err(CommandError::from_typed_service_error)?,
        )
        .map_err(command_error_from_db)?;
    }
    if plan.is_some() {
        verify_collection_access(ctx, factory, &revision)?;
    }
    Ok(revision.revision)
}

fn verify_collection_access(
    ctx: &ImportJobContext<'_>,
    factory: &dyn evidence_core::EvidenceReaderFactory,
    revision: &SourceRevisionRef,
) -> Result<(), CommandError> {
    let length = factory.logical_size().min(4096);
    if length == 0 {
        return Ok(());
    }
    let collection = ctx
        .import_config
        .collection_plan
        .as_ref()
        .ok_or_else(|| CommandError::internal("collection plan is missing"))?;
    let access = plan_target_access(
        TargetAccessRequest {
            target: AccessTarget::CollectionSource {
                collection,
                path: &ctx.import_config.source_path,
                kind: &ctx.import_config.kind,
            },
            source_revision: revision.clone(),
            ranges: Some(vec![ByteRange::new(0, length)
                .map_err(|error| CommandError::internal(error.to_string()))?]),
            intent: ReadIntent::MetadataInspection,
            priority: ReadPriority::Normal,
            deadline_unix_ms: None,
            reorderable: true,
            consumers: vec!["import-runtime-access".into()],
            verification: ReadVerification::ContentDigest,
            attempt: Some(StructureAttemptRef {
                attempt_id: ctx.job_id.0.clone(),
                owner: "import-runtime".into(),
            }),
        },
        ctx.options.cancel_token,
    )
    .map_err(|error| CommandError::from_service_error(error.to_string()))?;
    let mut view = open_verified_view(access, ctx.options.cancel_token)
        .map_err(|error| CommandError::from_service_error(error.to_string()))?;
    let mut buffer = vec![0u8; usize::try_from(length).unwrap_or(4096)];
    std::io::Read::read_exact(&mut view, &mut buffer)
        .map_err(|error| CommandError::from_service_error(error.to_string()))?;
    Ok(())
}

pub(super) fn publish_observations(
    ctx: &ImportJobContext<'_>,
    source: &domain::DataSource,
    receipt: &str,
    case_conn: &rusqlite::Connection,
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
    require_current(case_conn, identity)?;
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
        case_conn,
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
