use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use std::{io::Read, path::Path};
use transport::{
    dto::{ContentIndexCoverageDto, ContentIndexSummaryDto, EvidenceTaskRequestDto},
    CommandError,
};
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    source_revision: String,
    summary: ContentIndexSummaryDto,
    index_generation: String,
}
pub fn get(
    conn: &rusqlite::Connection,
    root: &Path,
    case: &CaseId,
    id: &str,
) -> Result<ContentIndexCoverageDto, CommandError> {
    if id.is_empty() || id.len() > 256 {
        return Err(CommandError::invalid_input("Invalid index source"));
    }
    let source = crate::source_db::open_ready_source_read_only_by_id(
        conn,
        root,
        case,
        &DataSourceId(id.into()),
    )
    .map_err(CommandError::from_service_error)?;
    let path = crate::source_db::registered_source_index_dir(conn, root, &source.data_source_id)
        .map_err(CommandError::from_service_error)?;
    let expected = FileRepo::new(&source.connection)
        .count_by_data_source(&source.data_source_id)
        .map_err(CommandError::from_service_error)?;
    let manifest = path.join("content-coverage.json");
    if !manifest
        .try_exists()
        .map_err(CommandError::from_service_error)?
    {
        return Ok(ContentIndexCoverageDto {
            summary: ContentIndexSummaryDto {
                expected,
                ..Default::default()
            },
            stale: false,
            source_revision: None,
        });
    }
    crate::source_db::safe_existing_case_path(root, &manifest)
        .map_err(CommandError::from_service_error)?;
    let mut bytes = Vec::new();
    std::fs::File::open(&manifest)
        .map_err(CommandError::from_service_error)?
        .take(16385)
        .read_to_end(&mut bytes)
        .map_err(CommandError::from_service_error)?;
    if bytes.len() > 16384 {
        return Err(CommandError::parser("Index coverage manifest is oversized"));
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|_| CommandError::parser("Index coverage manifest is corrupt"))?;
    let index = search::SearchIndex::open(&path).map_err(super::index_error)?;
    let current = crate::evidence_tasks::revision(
        conn,
        root,
        case,
        &EvidenceTaskRequestDto::ContentIndex {
            data_source_id: id.into(),
        },
    )?;
    let stale = current != manifest.source_revision
        || index.generation() != manifest.index_generation
        || expected != manifest.summary.expected
        || index.document_count().map_err(super::index_error)? != expected;
    Ok(ContentIndexCoverageDto {
        summary: manifest.summary,
        stale,
        source_revision: Some(manifest.source_revision),
    })
}
