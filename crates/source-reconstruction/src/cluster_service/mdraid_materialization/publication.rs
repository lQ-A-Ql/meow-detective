use std::path::Path;

use domain::{CaseId, DataSource};
use evidence_access::mdraid::{mdraid_array_fingerprint, MdRaidPlanError};

use crate::cluster_service::structure_publication::{
    self, StructureBuildRequest, StructurePublicationError, StructurePublicationValidator,
};

use super::build::CatalogBuild;

pub(super) struct MdRaidValidator;

impl StructurePublicationValidator for MdRaidValidator {
    fn validate(
        &self,
        conn: &rusqlite::Connection,
        case_id: &CaseId,
        source: &DataSource,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        super::inputs::validate(conn, source).map_err(map_error)?;
        let plan = evidence_access::mdraid::read_mdraid_descriptor(&source.source_path)
            .map_err(map_error)?;
        if mdraid_array_fingerprint(&plan).map_err(map_error)? != fingerprint {
            return Err(StructurePublicationError::InvalidPublication);
        }
        super::object_version::validate(conn, &case_id.0, &source.id.0, fingerprint)
            .map_err(map_error)?;
        Ok(())
    }
}

pub(super) fn seal_catalog(
    request: &CatalogBuild<'_>,
    conn: &rusqlite::Connection,
) -> Result<String, MdRaidPlanError> {
    let validator = MdRaidValidator;
    structure_publication::seal_catalog(&build_request(request, &validator), conn)
        .map_err(MdRaidPlanError::from)
}

pub(super) fn publish_build(
    request: &CatalogBuild<'_>,
    digest: &str,
) -> Result<(), MdRaidPlanError> {
    super::ensure_not_cancelled(request.cancel)?;
    let validator = MdRaidValidator;
    structure_publication::publish_build(&build_request(request, &validator), digest)
        .map_err(MdRaidPlanError::from)
}

pub(super) fn verify_publication(
    conn: &rusqlite::Connection,
    case_root: &Path,
    source: &DataSource,
    fingerprint: &str,
) -> Result<(), MdRaidPlanError> {
    let case_id = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
        .case_id(&source.id)?;
    let validator = MdRaidValidator;
    structure_publication::verify_publication(
        conn,
        case_root,
        source,
        fingerprint,
        &case_id,
        &validator,
    )
    .map_err(MdRaidPlanError::from)
}

pub(super) fn recover_prepared_publication(
    request: &CatalogBuild<'_>,
) -> Result<bool, MdRaidPlanError> {
    let validator = MdRaidValidator;
    structure_publication::recover_prepared_publication(&build_request(request, &validator))
        .map_err(MdRaidPlanError::from)
}

fn build_request<'a>(
    request: &'a CatalogBuild<'a>,
    validator: &'a MdRaidValidator,
) -> StructureBuildRequest<'a> {
    StructureBuildRequest {
        case_conn: request.case_conn,
        case_root: request.case_root,
        case_id: request.case_id,
        source: request.source,
        fingerprint: request.fingerprint,
        attempt: request.attempt,
        validator,
        cancel: request.cancel,
    }
}

fn map_error(error: MdRaidPlanError) -> StructurePublicationError {
    match error {
        MdRaidPlanError::Database(error) => StructurePublicationError::Database(error),
        MdRaidPlanError::Storage(error) => StructurePublicationError::Storage(error),
        MdRaidPlanError::Generation(error) => StructurePublicationError::Generation(error),
        MdRaidPlanError::Io(error) => StructurePublicationError::Io(error),
        MdRaidPlanError::Cancelled => StructurePublicationError::Cancelled,
        other => StructurePublicationError::Validation(other.to_string()),
    }
}

impl From<StructurePublicationError> for MdRaidPlanError {
    fn from(error: StructurePublicationError) -> Self {
        match error {
            StructurePublicationError::Database(error) => Self::Database(error),
            StructurePublicationError::Storage(error) => Self::Storage(error),
            StructurePublicationError::Generation(error) => Self::Generation(error),
            StructurePublicationError::Io(error) => Self::Io(error),
            StructurePublicationError::InvalidPublication
            | StructurePublicationError::Validation(_) => Self::InvalidPublication,
            StructurePublicationError::Cancelled => Self::Cancelled,
        }
    }
}
