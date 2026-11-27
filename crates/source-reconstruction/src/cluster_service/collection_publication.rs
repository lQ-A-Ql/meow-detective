use std::sync::atomic::{AtomicBool, Ordering};

use domain::{structure::CollectionMemberVerification, CaseId, DataSourceId};
use evidence_access::datasource_service::ImageCollectionPlan;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    source_meta_repo::{SourceMetaRepo, COLLECTION_MEMBER_VERIFICATION_KEY},
    source_revision_repo::SourceRevisionRepo,
};

use super::{ClusterServiceError, LinuxEvidenceSetImportPlan, Result};

pub fn validate_collection_publication(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    collection: &LinuxEvidenceSetImportPlan,
    plan: &ImageCollectionPlan,
    cancel: &AtomicBool,
) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err(ClusterServiceError::Cancelled);
    }
    plan.validate_snapshot()?;
    let transaction = conn.unchecked_transaction()?;
    let members = super::member_binding::ready_import_members(
        &transaction,
        case_id,
        &collection.import_set_id,
    )?;
    if members.len() != collection.members.len() {
        return Err(ClusterServiceError::IncompleteImportSet);
    }
    let sources = DataSourceRepo::new(&transaction).find_by_case(case_id)?;
    let revisions = SourceRevisionRepo::new(&transaction);
    let fingerprint = plan.content_fingerprint();
    for (index, source_id) in members {
        if cancel.load(Ordering::Acquire) {
            return Err(ClusterServiceError::Cancelled);
        }
        let member = collection
            .members
            .iter()
            .find(|member| member.member_index == index)
            .ok_or(ClusterServiceError::IncompleteImportSet)?;
        let source = sources
            .iter()
            .find(|source| source.id.0 == source_id)
            .ok_or(ClusterServiceError::IncompleteImportSet)?;
        if source.kind != member.source_kind
            || source.source_path.canonicalize()? != member.source_path.canonicalize()?
        {
            return Err(ClusterServiceError::IncompleteImportSet);
        }
        let (reference, content) = revisions.current(&case_id.0, &source_id)?;
        revisions.require_current(&reference)?;
        let expected = plan.source_content(&member.source_path, &member.source_kind)?;
        if content.logical_size != expected.logical_size
            || content.logical_sha256.as_deref() != Some(expected.logical_sha256.as_str())
        {
            return Err(ClusterServiceError::IncompleteImportSet);
        }
        let source_conn = source_kernel::source_db::open_registered_source_db_read_only(
            &transaction,
            case_root,
            &DataSourceId(source_id),
        )?;
        let json = SourceMetaRepo::new(&source_conn)
            .read(COLLECTION_MEMBER_VERIFICATION_KEY)?
            .ok_or(ClusterServiceError::IncompleteImportSet)?;
        let verified: CollectionMemberVerification = serde_json::from_str(&json)?;
        if verified.source != reference || verified.collection_fingerprint != fingerprint {
            return Err(ClusterServiceError::IncompleteImportSet);
        }
    }
    plan.validate_snapshot()?;
    transaction.commit()?;
    Ok(())
}
