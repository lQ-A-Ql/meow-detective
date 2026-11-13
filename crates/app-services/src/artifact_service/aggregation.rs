use std::path::Path;

use domain::{CaseId, DataSourceId};
use persistence_source::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;
use transport::dto::ArtifactRowDto;

use crate::source_db;

use super::{source_routing::artifact_to_source_dto, ArtifactServiceError};

const MAX_CASE_ARTIFACT_ROWS: usize = 1_000;

mod pagination;

pub use pagination::{
    get_artifact_rows_page_for_case, get_artifact_rows_page_with_cursor_for_case,
};

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
        if let Some(source) = open_complete_fjall_source(case_root, &source_id)? {
            let snapshot = source.snapshot();
            let mut cursor = None;
            let mut projected = false;
            loop {
                let page = match family {
                    Some(family) => source.artifact_list_by_type(
                        &snapshot,
                        &source_id.0,
                        family,
                        cursor.as_deref(),
                        persistence_source::fjall::ScanDirection::Forward,
                        256,
                    ),
                    None => source.artifact_list_by_source(
                        &snapshot,
                        &source_id.0,
                        cursor.as_deref(),
                        persistence_source::fjall::ScanDirection::Forward,
                        256,
                    ),
                }
                .map_err(|error| ArtifactServiceError::Other(error.to_string()))?;
                for record in page.records {
                    projected = true;
                    rows.push(artifact_record_to_dto(record, &source_id));
                }
                trim_artifact_rows(&mut rows);
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            if projected {
                continue;
            }
        }
        for artifact in ArtifactRepo::new(&source_conn).list_by_family(family)? {
            rows.push(artifact_to_source_dto(&artifact, &source_id));
            trim_artifact_rows(&mut rows);
        }
    }
    trim_artifact_rows(&mut rows);
    Ok(rows)
}

fn trim_artifact_rows(rows: &mut Vec<ArtifactRowDto>) {
    rows.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    rows.truncate(MAX_CASE_ARTIFACT_ROWS);
}

pub(crate) fn artifact_record_to_dto(
    record: persistence_source::fjall::ArtifactRecord,
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

pub(super) fn open_complete_fjall_source(
    case_root: &Path,
    source_id: &DataSourceId,
) -> Result<Option<crate::source_db::FjallReadOnlySourceStorageHandle>, ArtifactServiceError> {
    let source = source_db::open_registered_fjall_source_read_only(case_root, source_id)
        .map_err(|error| ArtifactServiceError::Other(error.to_string()))?;
    let snapshot = source.snapshot();
    source
        .require_projection_complete(&snapshot, &source_id.0)
        .map_err(|error| ArtifactServiceError::Other(error.to_string()))?;
    Ok(Some(source))
}
