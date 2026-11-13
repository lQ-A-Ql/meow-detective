use std::{collections::BTreeMap, path::Path};

use domain::{CaseId, DataSourceId, DataSourcePlatform};
use persistence_source::runtime::StorageRuntimeError;
use rusqlite::Connection;
use transport::dto::FamilyCountDto;

use crate::source_db;

use super::ArtifactServiceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceArtifactFamilyCount {
    pub data_source_id: DataSourceId,
    pub platform: DataSourcePlatform,
    pub family: String,
    pub count: u64,
}

pub fn get_artifact_families_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<Vec<String>, ArtifactServiceError> {
    Ok(
        get_artifact_family_counts_for_case(case_conn, case_root, case_id)?
            .into_iter()
            .map(|count| count.family)
            .collect(),
    )
}

pub fn get_artifact_family_counts_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<Vec<FamilyCountDto>, ArtifactServiceError> {
    let mut counts = BTreeMap::<String, u64>::new();
    for source_count in
        get_source_attributed_artifact_family_counts_for_case(case_conn, case_root, case_id)?
    {
        let total = counts.entry(source_count.family).or_default();
        *total = total.checked_add(source_count.count).ok_or_else(|| {
            ArtifactServiceError::Other("artifact family count overflowed".to_owned())
        })?;
    }
    Ok(counts
        .into_iter()
        .map(|(family, count)| FamilyCountDto { family, count })
        .collect())
}

pub(crate) fn get_source_attributed_artifact_family_counts_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<Vec<SourceArtifactFamilyCount>, ArtifactServiceError> {
    let mut counts = Vec::new();
    for (source, storage) in source_db::ready_data_sources(case_conn, case_id)? {
        let platform = DataSourcePlatform::parse_explicit(&storage.platform)
            .map_err(|error| ArtifactServiceError::Unsupported(error.to_string()))?;
        let handle = source_db::open_registered_fjall_source_read_only(case_root, &source.id)
            .map_err(|error| ArtifactServiceError::Other(error.to_string()))?;
        let snapshot = handle.snapshot();
        handle
            .require_projection_complete(&snapshot, &source.id.0)
            .map_err(|error| ArtifactServiceError::Other(error.to_string()))?;
        let source_counts = handle
            .artifact_type_counts(&snapshot, &source.id.0)
            .map_err(|error| match error {
                StorageRuntimeError::SourceNotReady(_) => {
                    ArtifactServiceError::InvalidInput(format!(
                        "artifact type-count index is not ready for source {}; re-import the data source",
                        source.id.0
                    ))
                }
                other => ArtifactServiceError::Other(other.to_string()),
            })?;
        counts.extend(
            source_counts
                .into_iter()
                .map(|(family, count)| SourceArtifactFamilyCount {
                    data_source_id: source.id.clone(),
                    platform,
                    family,
                    count,
                }),
        );
    }
    counts.sort_by(|left, right| {
        left.data_source_id
            .0
            .cmp(&right.data_source_id.0)
            .then_with(|| left.family.cmp(&right.family))
    });
    Ok(counts)
}
