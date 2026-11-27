use std::path::Path;

use domain::{CaseId, DataSource, DataSourceId};
use persistence_source::publication::catalog_publication::{
    CatalogProjection, StructureBuildRequest, StructurePublicationAttempt,
    StructurePublicationError, StructurePublicationValidator,
};
use persistence_sqlite::repositories::{
    ceph_fs_lineage_repo::{CephFsDerivedLineageAggregate, CephFsDerivedLineageRepo},
    source_revision_repo::SourceRevisionRepo,
};

use super::{
    catalog::{
        catalog_input_fingerprint, complete_catalog, fail_catalog_message, refresh_catalog,
        CatalogAttempt,
    },
    recovery::validate_published_database,
    CephFsSourceError, MaterializedCephFsSource,
};

pub(super) struct CephFsPublicationAttempt<'a> {
    pub(super) attempt: &'a CatalogAttempt,
    pub(super) summary: &'a MaterializedCephFsSource,
}

impl StructurePublicationAttempt for CephFsPublicationAttempt<'_> {
    fn attempt_id(&self) -> &str {
        &self.attempt.attempt_id
    }

    fn owner_id(&self) -> &str {
        &self.attempt.owner_id
    }

    fn refresh(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        _fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        refresh_catalog(conn, source, self.attempt)
            .map_err(|error| StructurePublicationError::Validation(error.to_string()))
    }

    fn finish_ready(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        _fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        complete_catalog(conn, source, self.attempt, self.summary)
            .map_err(|error| StructurePublicationError::Validation(error.to_string()))
    }

    fn finish_failed(
        &self,
        conn: &rusqlite::Connection,
        source: &DataSourceId,
        _fingerprint: &str,
        error: &str,
        cancelled: bool,
    ) -> Result<(), StructurePublicationError> {
        fail_catalog_message(conn, source, self.attempt, error, cancelled)
            .map_err(|error| StructurePublicationError::Validation(error.to_string()))
    }
}

pub(super) struct CephFsPublicationValidator<'a> {
    pub(super) case_root: &'a Path,
    pub(super) lineage: &'a CephFsDerivedLineageAggregate,
}

impl StructurePublicationValidator for CephFsPublicationValidator<'_> {
    fn validate(
        &self,
        conn: &rusqlite::Connection,
        case_id: &CaseId,
        source: &DataSource,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        if catalog_input_fingerprint(&self.lineage.lineage.lineage_fingerprint) != fingerprint {
            return Err(StructurePublicationError::InvalidPublication);
        }
        if CephFsDerivedLineageRepo::new(conn)
            .find_by_data_source(&source.id.0)?
            .as_ref()
            != Some(self.lineage)
        {
            return Err(StructurePublicationError::InvalidPublication);
        }
        SourceRevisionRepo::new(conn)
            .require_current_inputs(&case_id.0, &source.id.0)
            .map_err(StructurePublicationError::Database)?;
        let (_, revision) = SourceRevisionRepo::new(conn).current(&case_id.0, &source.id.0)?;
        if revision.metadata_identity.as_deref()
            != Some(self.lineage.lineage.namespace_projection_sha256.as_str())
        {
            return Err(StructurePublicationError::InvalidPublication);
        }
        let mut expected = self
            .lineage
            .pools
            .iter()
            .flat_map(|pool| pool.sources.iter())
            .map(|source| source.source_data_source_id.clone())
            .chain(
                self.lineage
                    .map_provenance
                    .iter()
                    .map(|source| source.source_data_source_id.clone()),
            )
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
            let published = validate_published_database(self.case_root, conn, source, self.lineage)
                .map_err(|error| StructurePublicationError::Validation(error.to_string()))?;
            if published.is_none() {
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
) -> StructureBuildRequest<'a> {
    StructureBuildRequest {
        case_conn,
        case_root,
        case_id,
        source,
        fingerprint,
        attempt,
        validator,
        projection: CatalogProjection::SourceOnly,
        cancel,
    }
}

pub(super) fn map_error(error: StructurePublicationError) -> CephFsSourceError {
    match error {
        StructurePublicationError::Database(error) => CephFsSourceError::Database(error),
        StructurePublicationError::Storage(error) => {
            CephFsSourceError::Database(persistence_sqlite::DbError::System(error.to_string()))
        }
        StructurePublicationError::Generation(error) => {
            CephFsSourceError::Database(persistence_sqlite::DbError::System(error.to_string()))
        }
        StructurePublicationError::Io(error) => CephFsSourceError::Io(error),
        StructurePublicationError::InvalidPublication
        | StructurePublicationError::Validation(_) => CephFsSourceError::StalePublication,
        StructurePublicationError::Cancelled => CephFsSourceError::Cancelled,
    }
}
