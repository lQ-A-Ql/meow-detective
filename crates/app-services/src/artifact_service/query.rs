use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;
use transport::dto::{ArtifactRowDto, FamilyCountDto};

use super::ArtifactServiceError;

pub fn get_artifact_families_from_db(
    conn: &Connection,
) -> Result<Vec<String>, ArtifactServiceError> {
    ArtifactRepo::new(conn)
        .families()
        .map_err(ArtifactServiceError::from)
}

pub fn get_artifact_rows_from_db(
    conn: &Connection,
    family: Option<&str>,
) -> Result<Vec<ArtifactRowDto>, ArtifactServiceError> {
    Ok(ArtifactRepo::new(conn)
        .list_by_family(family)?
        .iter()
        .map(artifact_to_dto)
        .collect())
}

pub fn get_artifact_row_by_id(
    conn: &Connection,
    artifact_id: &str,
) -> Result<Option<ArtifactRowDto>, ArtifactServiceError> {
    Ok(ArtifactRepo::new(conn)
        .find_by_id(artifact_id)?
        .as_ref()
        .map(artifact_to_dto))
}

pub fn get_artifact_family_counts(
    conn: &Connection,
) -> Result<Vec<FamilyCountDto>, ArtifactServiceError> {
    Ok(ArtifactRepo::new(conn)
        .count_by_family()?
        .into_iter()
        .map(|(family, count)| FamilyCountDto { family, count })
        .collect())
}

pub(super) fn artifact_to_dto(artifact: &domain::Artifact) -> ArtifactRowDto {
    ArtifactRowDto {
        id: artifact.id.0.clone(),
        artifact_type: artifact.family.clone(),
        title: artifact.title.clone(),
        summary: artifact.summary.clone(),
        source_object_id: artifact
            .source_object_id
            .as_ref()
            .map(|source_id| source_id.0.clone()),
        extractor_id: artifact.extractor_id.clone(),
        extractor_version: artifact.extractor_version.clone(),
        confidence: artifact.confidence,
        source_attribution: artifact.source_attribution.clone(),
        created_at: artifact.created_at.to_rfc3339(),
        attrs: artifact.attrs.clone(),
    }
}
