use std::sync::atomic::{AtomicBool, Ordering};

use domain::{CaseId, DataSource, DataSourceId};
use evidence_access::mdraid::{
    discover_mdraid_from_collection, mdraid_array_fingerprint, open_mdraid_reader,
    verify_mdraid_source_content, MdRaidArrayPlan, MdRaidPlanError,
};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    processing_phase_repo::{DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseClaim},
};

use evidence_access::datasource_service::ImageCollectionPlan;

mod build;
mod catalog;
mod phase;
mod publication;
mod registration;

pub fn materialize_mdraid_array(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &MdRaidArrayPlan,
) -> Result<DataSource, MdRaidPlanError> {
    materialize_mdraid_array_with_cancel(conn, case_root, case_id, plan, &AtomicBool::new(false))
}

fn materialize_mdraid_array_with_cancel(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &MdRaidArrayPlan,
    cancel: &AtomicBool,
) -> Result<DataSource, MdRaidPlanError> {
    ensure_not_cancelled(cancel)?;
    let source = registration::register_mdraid_array(conn, case_root, case_id, plan)?;
    materialize_mdraid_source_catalog_with_cancel(conn, case_root, &source.id, cancel)?;
    Ok(source)
}

pub fn materialize_mdraid_source_catalog(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source_id: &DataSourceId,
) -> Result<(), MdRaidPlanError> {
    materialize_mdraid_source_catalog_with_cancel(
        conn,
        case_root,
        source_id,
        &AtomicBool::new(false),
    )
}

pub fn materialize_mdraid_source_catalog_with_cancel(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source_id: &DataSourceId,
    cancel: &AtomicBool,
) -> Result<(), MdRaidPlanError> {
    ensure_not_cancelled(cancel)?;
    let repo = DataSourceRepo::new(conn);
    let case_id = repo.case_id(source_id)?;
    let source = repo
        .find_by_case(&case_id)?
        .into_iter()
        .find(|source| source.id == *source_id)
        .ok_or(MdRaidPlanError::InvalidPublication)?;
    if source.kind != domain::DataSourceKind::MdRaid {
        return Err(MdRaidPlanError::InvalidPublication);
    }
    let plan = evidence_access::mdraid::read_mdraid_descriptor(&source.source_path)?;
    let fingerprint = mdraid_array_fingerprint(&plan)?;
    if source.id != registration::source_id(&plan, &fingerprint) {
        return Err(MdRaidPlanError::InvalidPublication);
    }
    drop(open_mdraid_reader(&plan)?);
    verify_mdraid_source_content(&plan, cancel)?;
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
                return Err(MdRaidPlanError::InvalidPublication);
            }
            return publication::verify_publication(conn, case_root, &source, &fingerprint);
        }
        ProcessingPhaseClaim::Busy(_) => return Err(MdRaidPlanError::Busy),
        ProcessingPhaseClaim::Acquired(record) => phase::CatalogAttempt::new(record)?,
    };
    let result = build::build_and_publish(build::CatalogBuild {
        case_conn: conn,
        case_root,
        case_id: &case_id,
        source: &source,
        plan: &plan,
        fingerprint: &fingerprint,
        attempt: &attempt,
        cancel,
    });
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let message = error.to_string();
            let cancelled =
                cancel.load(Ordering::Acquire) || matches!(error, MdRaidPlanError::Cancelled);
            let transaction = conn.unchecked_transaction()?;
            DataSourceRepo::new(&transaction).update_import_state(
                source_id,
                if cancelled { "pending" } else { "failed" },
                Some(&message),
            )?;
            attempt.finish_failed(&transaction, source_id, &fingerprint, &message, cancelled)?;
            transaction.commit()?;
            Err(if cancelled {
                MdRaidPlanError::Cancelled
            } else {
                error
            })
        }
    }
}

pub fn materialize_linux_evidence_set_mdraid(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &ImageCollectionPlan,
) -> Result<Vec<DataSource>, MdRaidPlanError> {
    materialize_linux_evidence_set_mdraid_with_cancel(
        conn,
        case_root,
        case_id,
        plan,
        &AtomicBool::new(false),
    )
}

pub fn materialize_linux_evidence_set_mdraid_with_cancel(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    plan: &ImageCollectionPlan,
    cancel: &AtomicBool,
) -> Result<Vec<DataSource>, MdRaidPlanError> {
    ensure_not_cancelled(cancel)?;
    discover_mdraid_from_collection(plan, cancel)?
        .iter()
        .map(|array| materialize_mdraid_array_with_cancel(conn, case_root, case_id, array, cancel))
        .collect()
}

fn ensure_not_cancelled(cancel: &AtomicBool) -> Result<(), MdRaidPlanError> {
    if cancel.load(Ordering::Acquire) {
        Err(MdRaidPlanError::Cancelled)
    } else {
        Ok(())
    }
}
