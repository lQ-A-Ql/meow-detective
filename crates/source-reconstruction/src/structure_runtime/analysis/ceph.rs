mod osd;

use std::sync::atomic::Ordering;

use domain::structure::{
    CancellationScope, CapabilityKind, ConflictReason, EvidenceRequirement, MetadataFailure,
    SourceRevisionRef, TargetFailure,
};
use persistence_source::repositories::ceph_fs_namespace_repo::{
    CephFsNamespaceRepo, CephFsNamespaceRepoError, CEPHFS_NAMESPACE_DECODER_PROFILE,
    CEPHFS_NAMESPACE_SCHEMA_VERSION,
};
use persistence_sqlite::{
    repositories::{datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo},
    DbError,
};
use rusqlite::Connection;
use serde_json::{json, Value};

use super::super::{ExecutionContext, RuntimeError};

pub(super) fn analyze_osd(
    context: &ExecutionContext<'_>,
    source: &SourceRevisionRef,
) -> Result<Value, RuntimeError> {
    let control = open_control(context, source, CapabilityKind::MemberMetadata)?;
    let transaction = control.unchecked_transaction().map_err(DbError::from)?;
    let output = osd::analyze(&transaction, context, source)?;
    check_cancel(context)?;
    require_revision(context, source)?;
    transaction.commit().map_err(DbError::from)?;
    Ok(output)
}

pub(super) fn analyze_namespace(
    context: &ExecutionContext<'_>,
    source: &SourceRevisionRef,
    filesystem_identity: &str,
) -> Result<Value, RuntimeError> {
    if filesystem_identity.trim().is_empty() {
        return Err(missing(EvidenceRequirement::NativeVersionWitness));
    }
    let control = open_control(context, source, CapabilityKind::Namespace)?;
    let transaction = control.unchecked_transaction().map_err(DbError::from)?;
    let repository = CephFsNamespaceRepo::new(&transaction);
    let recorded = repository
        .find_manifest(filesystem_identity, &source.source_id)
        .map_err(namespace_error)?
        .ok_or_else(|| missing(EvidenceRequirement::NamespacePage))?;
    if recorded.schema_version != CEPHFS_NAMESPACE_SCHEMA_VERSION
        || recorded.decoder_profile != CEPHFS_NAMESPACE_DECODER_PROFILE
    {
        return Err(RuntimeError::Target(TargetFailure::UnsupportedFeature {
            capability: CapabilityKind::Namespace,
        }));
    }
    if !recorded.published && recorded.completeness == "incomplete" {
        return Err(missing(EvidenceRequirement::NamespacePage));
    }
    let manifest = repository
        .verify_published(filesystem_identity, &source.source_id)
        .map_err(namespace_error)?
        .ok_or_else(|| missing(EvidenceRequirement::NamespacePage))?;
    let output = json!({
        "sourceRevision": source,
        "filesystemIdentity": manifest.filesystem_identity,
        "filesystemId": manifest.filesystem_id,
        "fsmapEpoch": manifest.fsmap_epoch,
        "rootInode": manifest.root_inode,
        "inputSha256": manifest.input_sha256,
        "projectionSha256": manifest.projection_sha256,
        "schemaVersion": manifest.schema_version,
        "decoderProfile": manifest.decoder_profile,
        "completeness": manifest.completeness,
        "published": manifest.published,
        "entryCount": manifest.entry_count,
        "inodeCount": manifest.inode_count,
        "diagnosticCount": manifest.diagnostic_count,
    });
    check_cancel(context)?;
    require_revision(context, source)?;
    transaction.commit().map_err(DbError::from)?;
    Ok(output)
}

fn open_control(
    context: &ExecutionContext<'_>,
    source: &SourceRevisionRef,
    capability: CapabilityKind,
) -> Result<Connection, RuntimeError> {
    check_cancel(context)?;
    if source.case_id != context.request.case_id
        || !context
            .request
            .source_scope
            .iter()
            .any(|capture| capture.source == *source)
    {
        return Err(RuntimeError::InvalidDefinition);
    }
    let source_id = domain::DataSourceId(source.source_id.clone());
    let sources = DataSourceRepo::new(context.connection);
    let storage = sources
        .find_storage(&source_id)?
        .ok_or_else(|| missing(EvidenceRequirement::BackingVersion))?;
    if sources.case_id(&source_id)?.0 != source.case_id {
        return Err(RuntimeError::InvalidDefinition);
    }
    require_revision(context, source)?;
    if storage.storage_model != "source_db" {
        return Err(RuntimeError::Target(TargetFailure::UnsupportedFeature {
            capability,
        }));
    }
    if !matches!(storage.import_state.as_str(), "ready" | "ready_metadata")
        || storage.source_db_rel_path.is_none()
    {
        return Err(missing(EvidenceRequirement::ReferencedContent));
    }
    let control = source_kernel::source_db::open_registered_source_db_read_only(
        context.connection,
        context.case_root,
        &source_id,
    )
    .map_err(|error| match error {
        DbError::Io(error) if error.kind() == std::io::ErrorKind::NotFound => {
            missing(EvidenceRequirement::BackingVersion)
        }
        error => RuntimeError::Database(error),
    })?;
    if persistence_source::detail::source_id(&control)?.as_deref()
        != Some(source.source_id.as_str())
    {
        return Err(corrupt(MetadataFailure::InconsistentReferences));
    }
    Ok(control)
}

fn require_revision(
    context: &ExecutionContext<'_>,
    source: &SourceRevisionRef,
) -> Result<(), RuntimeError> {
    match SourceRevisionRepo::new(context.connection).current(&source.case_id, &source.source_id) {
        Ok((current, _)) if current == *source => Ok(()),
        Ok(_) => Err(RuntimeError::Target(TargetFailure::Conflicted {
            reason: ConflictReason::InputChanged,
        })),
        Err(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => {
            Err(missing(EvidenceRequirement::NativeVersionWitness))
        }
        Err(error) => Err(RuntimeError::Database(error)),
    }
}

fn namespace_error(error: CephFsNamespaceRepoError) -> RuntimeError {
    match error {
        CephFsNamespaceRepoError::Invalid(_) => corrupt(MetadataFailure::InvalidStructure),
        CephFsNamespaceRepoError::DeterminismConflict => {
            RuntimeError::Target(TargetFailure::Conflicted {
                reason: ConflictReason::NativeVersion,
            })
        }
        CephFsNamespaceRepoError::Database(error) => RuntimeError::Database(error),
    }
}

fn check_cancel(context: &ExecutionContext<'_>) -> Result<(), RuntimeError> {
    if context.cancel.load(Ordering::Acquire) {
        return Err(RuntimeError::Target(TargetFailure::Cancelled {
            scope: CancellationScope::Run,
        }));
    }
    Ok(())
}

fn missing(requirement: EvidenceRequirement) -> RuntimeError {
    RuntimeError::Target(TargetFailure::MissingEvidence { requirement })
}

fn corrupt(reason: MetadataFailure) -> RuntimeError {
    RuntimeError::Target(TargetFailure::CorruptMetadata { reason })
}
