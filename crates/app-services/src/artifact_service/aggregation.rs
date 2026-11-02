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

mod pagination;

pub use pagination::{
    get_artifact_rows_page_for_case, get_artifact_rows_page_with_cursor_for_case,
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
    for (source_id, source_conn) in
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?
    {
        if let Ok(source) = source_db::open_registered_fjall_source_read_only(case_root, &source_id)
        {
            let snapshot = source.snapshot();
            let mut cursor = None;
            loop {
                let page = source
                    .artifact_list_by_source(
                        &snapshot,
                        &source_id.0,
                        cursor.as_deref(),
                        persistence_fjall::ScanDirection::Forward,
                        256,
                    )
                    .map_err(|error| ArtifactServiceError::Other(error.to_string()))?;
                for record in page.records {
                    families.insert(record.artifact_type, ());
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            continue;
        }
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
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?
    {
        if let Ok(source) = source_db::open_registered_fjall_source_read_only(case_root, &source_id)
        {
            let snapshot = source.snapshot();
            let mut cursor = None;
            loop {
                let page = match family {
                    Some(family) => source.artifact_list_by_type(
                        &snapshot,
                        &source_id.0,
                        family,
                        cursor.as_deref(),
                        persistence_fjall::ScanDirection::Forward,
                        256,
                    ),
                    None => source.artifact_list_by_source(
                        &snapshot,
                        &source_id.0,
                        cursor.as_deref(),
                        persistence_fjall::ScanDirection::Forward,
                        256,
                    ),
                }
                .map_err(|error| ArtifactServiceError::Other(error.to_string()))?;
                rows.extend(
                    page.records
                        .into_iter()
                        .map(|record| artifact_record_to_dto(record, &source_id)),
                );
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            continue;
        }
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
    rows.truncate(1_000);
    Ok(rows)
}

pub(crate) fn artifact_record_to_dto(
    record: persistence_fjall::ArtifactRecord,
    source_id: &DataSourceId,
) -> ArtifactRowDto {
    let attrs = record.attrs;
    let field = |name: &str| attrs.get(name).cloned().unwrap_or(serde_json::Value::Null);
    ArtifactRowDto {
        id: crate::source_db::encode_source_scoped_id(source_id, &record.id),
        artifact_type: record.artifact_type,
        title: field("title").as_str().unwrap_or_default().to_owned(),
        summary: field("summary").as_str().unwrap_or_default().to_owned(),
        source_object_id: record
            .source_object_id
            .map(|id| crate::source_db::encode_source_scoped_id(source_id, &id)),
        extractor_id: field("extractorId").as_str().map(str::to_owned),
        extractor_version: field("extractorVersion").as_str().map(str::to_owned),
        confidence: field("confidence").as_f64().map(|value| value as f32),
        source_attribution: field("sourceAttribution").as_str().map(str::to_owned),
        created_at: field("createdAt").as_str().unwrap_or_default().to_owned(),
        attrs: serde_json::from_value(field("attrs")).unwrap_or_default(),
    }
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
        let source_db = source_db::open_ready_source_read_only_by_id(
            case_conn, case_root, case_id, &source.id,
        )?;
        if let Ok(fjall) = source_db::open_registered_fjall_source_read_only(case_root, &source.id)
        {
            let snapshot = fjall.snapshot();
            let mut cursor = None;
            let mut by_family = BTreeMap::<String, u64>::new();
            loop {
                let page = fjall
                    .artifact_list_by_source(
                        &snapshot,
                        &source.id.0,
                        cursor.as_deref(),
                        persistence_fjall::ScanDirection::Forward,
                        256,
                    )
                    .map_err(|error| ArtifactServiceError::Other(error.to_string()))?;
                for record in page.records {
                    *by_family.entry(record.artifact_type).or_default() += 1;
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            counts.extend(
                by_family
                    .into_iter()
                    .map(|(family, count)| SourceArtifactFamilyCount {
                        data_source_id: source.id.clone(),
                        platform,
                        family,
                        count,
                    }),
            );
            continue;
        }
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
