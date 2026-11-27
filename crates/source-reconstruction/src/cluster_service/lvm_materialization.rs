use std::sync::atomic::{AtomicBool, Ordering};

use domain::{CaseId, DataSource, DataSourceId};
use evidence_access::lvm::{
    lvm_volume_fingerprint, open_lvm_volume_reader, LvmPlanError, LvmVolumePlan,
};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    processing_phase_repo::{DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseClaim},
};

use evidence_access::datasource_service::ImageCollectionPlan;

mod build;
mod catalog;
mod inputs;
mod object_version;
mod phase;
mod registration;
mod validator;

pub fn materialize_lvm_volume(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &LvmVolumePlan,
    inputs: &[domain::structure::SourceRevisionRef],
) -> Result<DataSource, LvmPlanError> {
    materialize_lvm_volume_with_cancel(
        conn,
        case_root,
        case_id,
        plan,
        inputs,
        &AtomicBool::new(false),
    )
}

pub fn materialize_lvm_volume_with_cancel(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &LvmVolumePlan,
    inputs: &[domain::structure::SourceRevisionRef],
    cancel: &AtomicBool,
) -> Result<DataSource, LvmPlanError> {
    ensure_not_cancelled(cancel)?;
    let source = registration::register_lvm_volume(conn, case_root, case_id, plan, inputs)?;
    materialize_lvm_source_catalog_with_cancel(conn, case_root, &source.id, cancel)?;
    Ok(source)
}

pub fn materialize_lvm_source_catalog(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source_id: &DataSourceId,
) -> Result<(), LvmPlanError> {
    materialize_lvm_source_catalog_with_cancel(conn, case_root, source_id, &AtomicBool::new(false))
}

pub fn materialize_lvm_source_catalog_with_cancel(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source_id: &DataSourceId,
    cancel: &AtomicBool,
) -> Result<(), LvmPlanError> {
    ensure_not_cancelled(cancel)?;
    let repo = DataSourceRepo::new(conn);
    let case_id = repo.case_id(source_id)?;
    let source = repo
        .find_by_case(&case_id)?
        .into_iter()
        .find(|source| source.id == *source_id)
        .ok_or(LvmPlanError::InvalidPublication)?;
    if source.kind != domain::DataSourceKind::LvmLogicalVolume {
        return Err(LvmPlanError::InvalidPublication);
    }
    let plan = evidence_access::lvm::read_lvm_descriptor(&source.source_path)?;
    let fingerprint = lvm_volume_fingerprint(&plan)?;
    let registered_inputs =
        persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(conn)
            .inputs(&case_id.0, &source.id.0)?;
    if source.id != registration::source_id(&plan, &fingerprint, &registered_inputs) {
        return Err(LvmPlanError::InvalidPublication);
    }
    inputs::validate(conn, &source)?;
    object_version::validate(conn, &case_id.0, &source.id.0, &fingerprint)?;
    drop(open_lvm_volume_reader(&plan, cancel)?);
    let owner = uuid::Uuid::new_v4().to_string();
    let claim = DataSourceProcessingPhaseRepo::new(conn).claim(
        &source.id,
        ProcessingPhase::Catalog,
        phase::VERSION,
        &fingerprint,
        &owner,
    )?;
    let attempt = match claim {
        ProcessingPhaseClaim::Ready(_) => {
            if !repo
                .find_storage(&source.id)?
                .is_some_and(|state| state.import_state == "ready")
            {
                return Err(LvmPlanError::InvalidPublication);
            }
            let validator = validator::LvmValidator { case_root };
            crate::cluster_service::structure_publication::verify_publication(
                conn,
                case_root,
                &source,
                &fingerprint,
                &case_id,
                &validator,
            )?;
            return Ok(());
        }
        ProcessingPhaseClaim::Busy(_) => return Err(LvmPlanError::Busy),
        ProcessingPhaseClaim::Acquired(record) => phase::LvmAttempt::new(record)?,
    };
    let validator = validator::LvmValidator { case_root };
    let result = build::build_and_publish(build::CatalogBuild {
        case_conn: conn,
        case_root,
        case_id: &case_id,
        source: &source,
        plan: &plan,
        fingerprint: &fingerprint,
        attempt: &attempt,
        validator: &validator,
        cancel,
    });
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            use crate::cluster_service::structure_publication::StructurePublicationAttempt;
            let message = error.to_string();
            let cancelled =
                cancel.load(Ordering::Acquire) || matches!(error, LvmPlanError::Cancelled);
            let transaction = conn.unchecked_transaction()?;
            DataSourceRepo::new(&transaction).update_import_state(
                source_id,
                if cancelled { "pending" } else { "failed" },
                Some(&message),
            )?;
            attempt.finish_failed(&transaction, source_id, &fingerprint, &message, cancelled)?;
            transaction.commit()?;
            Err(if cancelled {
                LvmPlanError::Cancelled
            } else {
                error
            })
        }
    }
}

pub fn materialize_linux_evidence_set_lvm(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    import: &super::LinuxEvidenceSetImportPlan,
    plan: &ImageCollectionPlan,
) -> Result<Vec<DataSource>, LvmPlanError> {
    materialize_linux_evidence_set_lvm_with_cancel(
        conn,
        case_root,
        case_id,
        import,
        plan,
        &AtomicBool::new(false),
    )
}

pub fn materialize_linux_evidence_set_lvm_with_cancel(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    import: &super::LinuxEvidenceSetImportPlan,
    plan: &ImageCollectionPlan,
    cancel: &AtomicBool,
) -> Result<Vec<DataSource>, LvmPlanError> {
    ensure_not_cancelled(cancel)?;
    evidence_access::lvm::discover_volumes_from_collection(plan, cancel)?
        .iter()
        .map(|volume| {
            let inputs = inputs::from_collection(conn, case_id, import, volume)?;
            materialize_lvm_volume_with_cancel(conn, case_root, case_id, volume, &inputs, cancel)
        })
        .collect()
}

fn ensure_not_cancelled(cancel: &AtomicBool) -> Result<(), LvmPlanError> {
    if cancel.load(Ordering::Acquire) {
        Err(LvmPlanError::Cancelled)
    } else {
        Ok(())
    }
}
