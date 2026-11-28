mod build;
mod registration;

use domain::{structure::SourceRevisionRef, CaseId, DataSource};
use evidence_access::{
    btrfs_pool::{BtrfsPoolPlan, BtrfsSourceError},
    datasource_service::ImageCollectionPlan,
};
use persistence_source::publication::{
    catalog_phase::CatalogPhaseAttempt,
    catalog_publication::{
        self as publication, CatalogProjection, StructureBuildRequest, StructurePublicationAttempt,
        StructurePublicationError, StructurePublicationValidator,
    },
};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    linux_import_set_repo::LinuxImportSetRepo,
    processing_phase_repo::{DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseClaim},
    source_revision_repo::SourceRevisionRepo,
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

const VERSION: u32 = 1;

pub fn materialize_btrfs_pool(
    conn: &rusqlite::Connection,
    root: &Path,
    case_id: &CaseId,
    plan: &BtrfsPoolPlan,
    inputs: &[SourceRevisionRef],
    cancel: &AtomicBool,
) -> Result<DataSource, BtrfsSourceError> {
    if cancel.load(Ordering::Acquire) {
        return Err(BtrfsSourceError::Cancelled);
    }
    let source = registration::register(conn, root, case_id, plan, inputs)?;
    let fingerprint = plan.fingerprint()?;
    let validator = Validator;
    let claim = DataSourceProcessingPhaseRepo::new(conn).claim(
        &source.id,
        ProcessingPhase::Catalog,
        VERSION,
        &fingerprint,
        &uuid::Uuid::new_v4().to_string(),
    )?;
    let attempt = match claim {
        ProcessingPhaseClaim::Ready(_) => {
            publication::verify_publication(
                conn,
                root,
                &source,
                &fingerprint,
                case_id,
                CatalogProjection::SourceOnly,
                &validator,
            )?;
            return Ok(source);
        }
        ProcessingPhaseClaim::Busy(_) => return Err(BtrfsSourceError::Identity),
        ProcessingPhaseClaim::Acquired(record) => CatalogPhaseAttempt::new(record, VERSION)?,
    };
    let request = StructureBuildRequest {
        case_conn: conn,
        case_root: root,
        case_id,
        source: &source,
        fingerprint: &fingerprint,
        attempt: &attempt,
        validator: &validator,
        projection: CatalogProjection::SourceOnly,
        cancel,
    };
    if let Err(error) = build::execute(&request, plan) {
        let cancelled = cancel.load(Ordering::Acquire);
        let transaction = conn
            .unchecked_transaction()
            .map_err(persistence_sqlite::DbError::from)?;
        attempt.finish_failed(
            &transaction,
            &source.id,
            &fingerprint,
            &error.to_string(),
            cancelled,
        )?;
        DataSourceRepo::new(&transaction).update_import_state(
            &source.id,
            if cancelled { "pending" } else { "failed" },
            Some(&error.to_string()),
        )?;
        transaction
            .commit()
            .map_err(persistence_sqlite::DbError::from)?;
        return Err(error);
    }
    Ok(source)
}

pub fn materialize_linux_evidence_set_btrfs(
    conn: &rusqlite::Connection,
    root: &Path,
    case_id: &CaseId,
    import: &super::LinuxEvidenceSetImportPlan,
    collection: &ImageCollectionPlan,
    cancel: &AtomicBool,
) -> Result<Vec<DataSource>, BtrfsSourceError> {
    let members = LinuxImportSetRepo::new(conn).find_members(&import.import_set_id)?;
    let coordinates = import
        .members
        .iter()
        .map(|member| Ok((member, member.source_path.canonicalize()?)))
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    let mut result = Vec::new();
    for plan in collection.btrfs_pools() {
        let mut inputs = Vec::new();
        for member in &plan.members {
            let canonical = Path::new(&member.content.source_path).canonicalize()?;
            let mut matching = coordinates.iter().filter(|(coordinate, path)| {
                coordinate.source_kind == member.content.source_kind && path == &canonical
            });
            let (coordinate, _) = matching.next().ok_or(BtrfsSourceError::Identity)?;
            if matching.next().is_some() {
                return Err(BtrfsSourceError::Identity);
            }
            let stored = members
                .iter()
                .find(|stored| stored.member_index == coordinate.member_index)
                .ok_or(BtrfsSourceError::Identity)?;
            if stored.source_kind != coordinate.source_kind.to_string()
                || Path::new(&stored.source_path) != coordinate.source_path
            {
                return Err(BtrfsSourceError::Identity);
            }
            let id = LinuxImportSetRepo::new(conn)
                .find_member_source_id(&case_id.0, &import.import_set_id, coordinate.member_index)?
                .ok_or(BtrfsSourceError::Identity)?;
            let reference = SourceRevisionRepo::new(conn).current(&case_id.0, &id)?.0;
            if !inputs.contains(&reference) {
                inputs.push(reference);
            }
        }
        result.push(materialize_btrfs_pool(
            conn, root, case_id, plan, &inputs, cancel,
        )?);
    }
    Ok(result)
}

struct Validator;
impl StructurePublicationValidator for Validator {
    fn validate(
        &self,
        conn: &rusqlite::Connection,
        case_id: &CaseId,
        source: &DataSource,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        let plan = evidence_access::btrfs_pool::read_descriptor(&source.source_path)
            .map_err(|error| StructurePublicationError::Validation(error.to_string()))?;
        if plan
            .fingerprint()
            .map_err(|error| StructurePublicationError::Validation(error.to_string()))?
            != fingerprint
        {
            return Err(StructurePublicationError::InvalidPublication);
        }
        let (reference, _) = SourceRevisionRepo::new(conn).current(&case_id.0, &source.id.0)?;
        evidence_access::btrfs_pool::validate_registered(conn, &reference, &plan)
            .map_err(|error| StructurePublicationError::Validation(error.to_string()))
    }
}
