use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use domain::{
    structure::{ReadIntent, ReadPriority},
    CaseId, DataSourceId, DataSourceKind,
};
use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo, datasource_repo::DataSourceRepo,
    source_revision_repo::SourceRevisionRepo,
};

#[derive(Debug, thiserror::Error)]
pub enum SourceProcessingError {
    #[error(transparent)]
    Database(#[from] persistence_sqlite::DbError),
    #[error(transparent)]
    Structure(#[from] crate::structure_analysis::StructureAnalysisError),
    #[error(transparent)]
    Access(#[from] evidence_access::structure_access::TargetAccessError),
    #[error(transparent)]
    Publication(
        #[from] persistence_source::publication::catalog_publication::StructurePublicationError,
    ),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("source does not provide admitted catalog processing")]
    Unsupported,
    #[error("source processing was cancelled")]
    Cancelled,
    #[error("source processing is incomplete: {failed} failed, {deferred} deferred, {unfinished} unfinished")]
    Incomplete {
        failed: usize,
        deferred: usize,
        unfinished: usize,
    },
}

impl transport::ServiceErrorCategory for SourceProcessingError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Cancelled => transport::ErrorCategory::Cancelled,
            Self::Unsupported => transport::ErrorCategory::Unsupported,
            _ => transport::ErrorCategory::Io,
        }
    }
}

pub fn finalize_source_processing_with_cancel(
    conn: &rusqlite::Connection,
    root: &Path,
    case_id: &CaseId,
    source: &DataSourceId,
    cancel: Arc<AtomicBool>,
) -> Result<(), SourceProcessingError> {
    if cancel.load(Ordering::Acquire) {
        return Err(SourceProcessingError::Cancelled);
    }
    let sources = DataSourceRepo::new(conn);
    if sources.case_id(source)? != *case_id
        || !matches!(
            sources.source_kind(source)?,
            DataSourceKind::MdRaid
                | DataSourceKind::LvmLogicalVolume
                | DataSourceKind::BtrfsPool
                | DataSourceKind::CephRbd
                | DataSourceKind::CephFs
        )
    {
        return Err(SourceProcessingError::Unsupported);
    }
    drop(
        source_kernel::source_db::open_ready_source_read_only_by_id(conn, root, case_id, source)
            .map_err(|error| error.into_db_error())?,
    );
    let publication = CatalogPublicationRepo::new(conn)
        .find(source)?
        .ok_or(SourceProcessingError::Unsupported)?;
    let seed = if sources.source_kind(source)? == DataSourceKind::CephRbd {
        ceph_reconstruction::ceph_reconstruction::load_lineage_fingerprint(conn, source)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?
    } else {
        publication.input_fingerprint.clone()
    };
    let reference = admit_processing(conn, root, case_id, source, &seed, &cancel)?;
    let report = crate::derived_source_service::finalizer::finalize_derived_source(
        conn, root, case_id, source, &seed, &cancel,
    );
    if cancel.load(Ordering::Acquire) {
        return Err(SourceProcessingError::Cancelled);
    }
    SourceRevisionRepo::new(conn).require_current(&reference)?;
    if !report.all_ready() {
        return Err(SourceProcessingError::Incomplete {
            failed: report.failed_count(),
            deferred: report.deferred_count(),
            unfinished: report.unfinished_count(),
        });
    }
    Ok(())
}

fn admit_processing(
    conn: &rusqlite::Connection,
    root: &Path,
    case_id: &CaseId,
    source: &DataSourceId,
    seed: &str,
    cancel: &AtomicBool,
) -> Result<domain::structure::SourceRevisionRef, SourceProcessingError> {
    let reference = SourceRevisionRepo::new(conn)
        .current(&case_id.0, &source.0)?
        .0;
    let verified = crate::structure_analysis::execute_structure_analysis(
        conn,
        root,
        &crate::structure_analysis::StructureAnalysisRequest {
            case_id: case_id.clone(),
            source_ids: vec![source.0.clone()],
            intent: ReadIntent::MetadataInspection,
            priority: ReadPriority::Normal,
            consumers: vec!["source-processing".into()],
            attempt: None,
            required_features: Default::default(),
        },
        cancel,
    )?;
    let scope =
        evidence_access::structure_access::acquire_registered_scope(conn, root, &reference)?;
    let lease = verified
        .permit
        .bind_publication(
            source_kernel::source_runtime::SourcePublicationLease::acquire_verified_scope(scope)?,
        )
        .map_err(|error| {
            std::io::Error::other(format!("source processing receipt binding failed: {error}"))
        })?;
    lease.validate().map_err(|error| {
        std::io::Error::other(format!(
            "source processing initial validation failed: {error}"
        ))
    })?;
    {
        let _fence = lease.fence_connection(conn);
        let transaction = conn
            .unchecked_transaction()
            .map_err(persistence_sqlite::DbError::from)?;
        crate::structure_facts::index_source_scopes(
            &transaction,
            case_id,
            &[(0, source.0.clone(), source.0.clone())],
        )?;
        crate::derived_source_service::finalizer::queue_post_catalog_phases(
            &transaction,
            source,
            seed,
        )?;
        transaction
            .commit()
            .map_err(persistence_sqlite::DbError::from)?;
    }
    Ok(reference)
}
