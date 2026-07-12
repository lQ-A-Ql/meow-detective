use std::path::Path;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;
use transport::dto::ArtifactRowDto;

use crate::source_db::{self, encode_source_scoped_id};

use super::{query::artifact_to_dto, ArtifactServiceError};

pub fn get_artifact_row_by_id_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    artifact_id: &str,
) -> Result<Option<ArtifactRowDto>, ArtifactServiceError> {
    let (source_id, local_id) = source_db::parse_source_scoped_id("Artifact id", artifact_id)
        .map_err(|error| {
            ArtifactServiceError::invalid_input(format!(
                "{error}; source database artifacts require ds:<dataSourceId>:<localId>"
            ))
        })?;
    let source = source_db::open_ready_source_by_id(case_conn, case_root, case_id, &source_id)?;
    Ok(ArtifactRepo::new(&source.connection)
        .find_by_id(&local_id)?
        .as_ref()
        .map(|artifact| artifact_to_source_dto(artifact, &source_id)))
}

pub(super) fn artifact_to_source_dto(
    artifact: &domain::Artifact,
    data_source_id: &DataSourceId,
) -> ArtifactRowDto {
    let mut dto = artifact_to_dto(artifact);
    dto.id = encode_source_scoped_id(data_source_id, &artifact.id.0);
    dto.source_object_id = artifact
        .source_object_id
        .as_ref()
        .map(|source_id| encode_source_scoped_id(data_source_id, &source_id.0));
    dto
}
