use std::path::Path;

use domain::{CaseId, DataSource, DataSourceId, DataSourceKind};
use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo, datasource_repo::DataSourceRepo,
    source_revision_repo::SourceRevisionRepo,
};

use super::{
    verify_publication, CatalogProjection, StructurePublicationError, StructurePublicationValidator,
};

struct RegisteredInputs;

impl StructurePublicationValidator for RegisteredInputs {
    fn validate(
        &self,
        conn: &rusqlite::Connection,
        case_id: &CaseId,
        source: &DataSource,
        _fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        SourceRevisionRepo::new(conn).require_current_inputs(&case_id.0, &source.id.0)?;
        Ok(())
    }
}

pub fn verify_registered_catalog(
    conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    source_id: &DataSourceId,
) -> Result<(), StructurePublicationError> {
    if !matches!(
        DataSourceRepo::new(conn).source_kind(source_id)?,
        DataSourceKind::CephFs
            | DataSourceKind::CephRbd
            | DataSourceKind::MdRaid
            | DataSourceKind::LvmLogicalVolume
    ) {
        return Ok(());
    }
    let source = DataSourceRepo::new(conn)
        .find_by_case(case_id)?
        .into_iter()
        .find(|source| source.id == *source_id)
        .ok_or(StructurePublicationError::InvalidPublication)?;
    let projection = match source.kind {
        DataSourceKind::CephFs => CatalogProjection::SourceOnly,
        DataSourceKind::CephRbd | DataSourceKind::MdRaid | DataSourceKind::LvmLogicalVolume => {
            CatalogProjection::Partition
        }
        _ => return Ok(()),
    };
    let publication = CatalogPublicationRepo::new(conn)
        .find(source_id)?
        .ok_or(StructurePublicationError::InvalidPublication)?;
    verify_publication(
        conn,
        case_root,
        &source,
        &publication.input_fingerprint,
        case_id,
        projection,
        &RegisteredInputs,
    )
}
