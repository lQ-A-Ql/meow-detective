use std::path::Path;

use domain::{CaseId, DataSource, DataSourceId};
use persistence_source::publication::catalog_publication::{
    CatalogProjection, StructurePublicationAttempt, StructurePublicationError,
    StructurePublicationValidator,
};
use persistence_sqlite::repositories::{
    ceph_rbd_lineage_repo::CephRbdLineageRepo, source_revision_repo::SourceRevisionRepo,
};

use super::DerivedSourceError;
use crate::derived_source_service::{
    catalog_manifest::{catalog_fingerprint_for_source, load_current_source_summary},
    finalizer::{
        complete_catalog_phase, defer_catalog_phase, fail_catalog_phase, queue_post_catalog_phases,
        refresh_catalog_claim, ProcessingPhaseAttempt,
    },
    MaterializedRbdSource,
};
use ceph_reconstruction::ceph_reconstruction::load_lineage_fingerprint;

pub(super) struct RbdPublicationAttempt<'a> {
    pub(super) attempt: &'a ProcessingPhaseAttempt,
    pub(super) summary: &'a MaterializedRbdSource,
    pub(super) lineage_fingerprint: &'a str,
}

impl StructurePublicationAttempt for RbdPublicationAttempt<'_> {
    fn attempt_id(&self) -> &str {
        self.attempt.attempt_id()
    }

    fn owner_id(&self) -> &str {
        self.attempt.owner_id()
    }

    fn refresh(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        _fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        refresh_catalog_claim(conn, source, self.lineage_fingerprint, self.attempt)
            .map_err(StructurePublicationError::Database)
    }

    fn finish_ready(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        _fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        complete_catalog_phase(
            conn,
            source,
            self.lineage_fingerprint,
            self.attempt,
            self.summary,
        )
        .map_err(StructurePublicationError::Database)?;
        queue_post_catalog_phases(conn, source, self.lineage_fingerprint)
            .map_err(StructurePublicationError::Database)
    }

    fn finish_failed(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        _fingerprint: &str,
        error: &str,
        cancelled: bool,
    ) -> Result<(), StructurePublicationError> {
        if cancelled {
            defer_catalog_phase(conn, source, self.lineage_fingerprint, self.attempt, error)
        } else {
            fail_catalog_phase(conn, source, self.lineage_fingerprint, self.attempt, error)
        }
        .map_err(StructurePublicationError::Database)
    }
}

pub(super) struct RbdPublicationValidator<'a> {
    pub(super) case_root: &'a Path,
}

impl StructurePublicationValidator for RbdPublicationValidator<'_> {
    fn validate(
        &self,
        conn: &rusqlite::Connection,
        case_id: &CaseId,
        source: &DataSource,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        let lineage_fingerprint = load_lineage_fingerprint(conn, &source.id)
            .map_err(|error| StructurePublicationError::Validation(error.to_string()))?;
        self.validate_with_fingerprints(conn, case_id, source, fingerprint, &lineage_fingerprint)
    }
}

impl RbdPublicationValidator<'_> {
    fn validate_with_fingerprints(
        &self,
        conn: &rusqlite::Connection,
        case_id: &CaseId,
        source: &DataSource,
        fingerprint: &str,
        lineage_fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        if catalog_fingerprint_for_source(lineage_fingerprint) != fingerprint {
            return Err(StructurePublicationError::InvalidPublication);
        }
        SourceRevisionRepo::new(conn)
            .require_current_inputs(&case_id.0, &source.id.0)
            .map_err(StructurePublicationError::Database)?;
        let (_, revision) = SourceRevisionRepo::new(conn).current(&case_id.0, &source.id.0)?;
        if revision.metadata_identity.as_deref() != Some(fingerprint) {
            return Err(StructurePublicationError::InvalidPublication);
        }
        let lineage = CephRbdLineageRepo::new(conn)
            .find_by_data_source(&source.id.0)
            .map_err(StructurePublicationError::Database)?
            .ok_or(StructurePublicationError::InvalidPublication)?;
        let mut expected = lineage
            .replicas
            .iter()
            .map(|replica| replica.source_data_source_id.as_str())
            .collect::<Vec<_>>();
        expected.sort_unstable();
        expected.dedup();
        let mut actual = SourceRevisionRepo::new(conn)
            .inputs(&case_id.0, &source.id.0)
            .map_err(StructurePublicationError::Database)?
            .into_iter()
            .map(|input| input.source_id)
            .collect::<Vec<_>>();
        actual.sort_unstable();
        actual.dedup();
        if actual != expected {
            return Err(StructurePublicationError::InvalidPublication);
        }
        let source_path = source_kernel::source_db::source_db_path(self.case_root, &source.id);
        if source_path.is_file() {
            let source_conn =
                persistence_source::control::opening::open_source_database_read_only(&source_path)?;
            if load_current_source_summary(&source_conn, lineage_fingerprint, source.clone())
                .map_err(map_derived_error)?
                .is_none()
            {
                return Err(StructurePublicationError::InvalidPublication);
            }
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn publication_request<'a>(
    case_conn: &'a rusqlite::Connection,
    case_root: &'a Path,
    case_id: &'a CaseId,
    source: &'a DataSource,
    fingerprint: &'a str,
    attempt: &'a dyn StructurePublicationAttempt,
    validator: &'a dyn StructurePublicationValidator,
    cancel: &'a std::sync::atomic::AtomicBool,
) -> persistence_source::publication::catalog_publication::StructureBuildRequest<'a> {
    persistence_source::publication::catalog_publication::StructureBuildRequest {
        case_conn,
        case_root,
        case_id,
        source,
        fingerprint,
        attempt,
        validator,
        projection: CatalogProjection::Partition,
        cancel,
    }
}

impl From<StructurePublicationError> for DerivedSourceError {
    fn from(error: StructurePublicationError) -> Self {
        match error {
            StructurePublicationError::Database(error) => DerivedSourceError::Database(error),
            StructurePublicationError::Cancelled => DerivedSourceError::ProcessingCancelled,
            StructurePublicationError::Io(error) => DerivedSourceError::Io(error),
            StructurePublicationError::Storage(error) => {
                DerivedSourceError::InconsistentState(error.to_string())
            }
            StructurePublicationError::Generation(error) => {
                DerivedSourceError::InconsistentState(error.to_string())
            }
            StructurePublicationError::InvalidPublication => DerivedSourceError::InconsistentState(
                "structure publication is inconsistent".to_string(),
            ),
            StructurePublicationError::Validation(error) => {
                DerivedSourceError::InconsistentState(error)
            }
        }
    }
}

fn map_derived_error(error: DerivedSourceError) -> StructurePublicationError {
    match error {
        DerivedSourceError::Database(error) => StructurePublicationError::Database(error),
        DerivedSourceError::ProcessingCancelled => StructurePublicationError::Cancelled,
        other => StructurePublicationError::Validation(other.to_string()),
    }
}
