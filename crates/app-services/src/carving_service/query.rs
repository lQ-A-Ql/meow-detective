use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo,
    source_meta_repo::{SourceMetaRepo, ARTIFACT_CURSOR_REVISION_KEY},
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use transport::{
    dto::{CarvedArtifactPageDto, CarvedSearchRequestDto},
    paging::{decode_opaque_cursor, encode_opaque_cursor},
    CommandError,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    kind: String,
    case: String,
    source: String,
    source_revision: String,
    query: String,
    revision: u64,
    after: String,
}

pub fn search_carved_artifacts(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    request: &CarvedSearchRequestDto,
) -> Result<CarvedArtifactPageDto, CommandError> {
    if request.query.len() > 128 || request.cursor.as_ref().is_some_and(|c| c.len() > 4096) {
        return Err(CommandError::invalid_input("Invalid carving search"));
    }
    let source_revision =
        super::access::source_revision(conn, root, case, &request.data_source_id)?;
    let source = crate::source_db::open_ready_source_read_only_by_id(
        conn,
        root,
        case,
        &DataSourceId(request.data_source_id.clone()),
    )
    .map_err(CommandError::from_service_error)?;
    let transaction = source
        .connection
        .unchecked_transaction()
        .map_err(CommandError::from_service_error)?;
    let revision = SourceMetaRepo::new(&transaction)
        .read_revision(ARTIFACT_CURSOR_REVISION_KEY)
        .map_err(CommandError::from_service_error)?;
    let mut cursor = Cursor {
        kind: "carving-v1".into(),
        case: case.0.clone(),
        source: request.data_source_id.clone(),
        source_revision,
        query: request.query.clone(),
        revision,
        after: String::new(),
    };
    if let Some(token) = &request.cursor {
        let decoded: Cursor =
            decode_opaque_cursor(token).map_err(CommandError::from_service_error)?;
        if decoded.kind != cursor.kind
            || decoded.case != cursor.case
            || decoded.source != cursor.source
            || decoded.source_revision != cursor.source_revision
            || decoded.query != cursor.query
            || decoded.revision != revision
            || decoded.after.len() > 256
        {
            return Err(CommandError::invalid_input(
                "Carving search cursor expired or changed scope",
            ));
        }
        cursor = decoded;
    }
    let mut artifacts = ArtifactRepo::new(&transaction)
        .carving_page(&cursor.source, &cursor.after, &cursor.query)
        .map_err(CommandError::from_service_error)?;
    let more = artifacts.len() > 100;
    artifacts.truncate(100);
    let next_cursor = if more {
        cursor.after = artifacts
            .last()
            .ok_or_else(|| CommandError::internal("Carving page boundary missing"))?
            .id
            .0
            .clone();
        Some(encode_opaque_cursor(&cursor).map_err(CommandError::from_service_error)?)
    } else {
        None
    };
    let items = artifacts
        .iter()
        .map(|artifact| super::access::dto(artifact, &DataSourceId(cursor.source.clone())))
        .collect::<Result<_, _>>()?;
    Ok(CarvedArtifactPageDto { items, next_cursor })
}
