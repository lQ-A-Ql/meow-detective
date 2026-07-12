use std::{collections::BTreeMap, path::Path};

use domain::{CaseId, DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;
use transport::dto::{ArtifactRowDto, FamilyCountDto};

use crate::source_db;

use super::{
    query::get_artifact_families_from_db, source_routing::artifact_to_source_dto,
    ArtifactServiceError,
};

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
    let mut families = BTreeMap::<String, ()>::new();
    for (_, source_conn) in source_db::open_ready_source_connections(case_conn, case_root, case_id)?
    {
        for family in get_artifact_families_from_db(&source_conn)? {
            families.insert(family, ());
        }
    }
    Ok(families.into_keys().collect())
}

pub fn get_artifact_rows_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    family: Option<&str>,
) -> Result<Vec<ArtifactRowDto>, ArtifactServiceError> {
    let mut rows = Vec::new();
    for (source_id, source_conn) in
        source_db::open_ready_source_connections(case_conn, case_root, case_id)?
    {
        rows.extend(
            ArtifactRepo::new(&source_conn)
                .list_by_family(family)?
                .iter()
                .map(|artifact| artifact_to_source_dto(artifact, &source_id)),
        );
    }
    rows.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    rows.truncate(1000);
    Ok(rows)
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
        *counts.entry(source_count.family).or_default() += source_count.count;
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
        let source_db =
            source_db::open_ready_source_by_id(case_conn, case_root, case_id, &source.id)?;
        counts.extend(
            ArtifactRepo::new(&source_db.connection)
                .count_by_family()?
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
