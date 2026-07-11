use domain::DataSourceId;
use rusqlite::Connection;
use search::{SearchHit, SearchIndex, SearchResult};
use std::path::Path;
use transport::dto::{SearchHighlightDto, SearchHitDto, SearchResultPageDto, SearchSnippetDto};

use super::SearchError;
use crate::source_db::{
    encode_source_scoped_id, safe_case_relative_path, safe_existing_case_path, source_index_dir,
};

const MAX_CASE_SEARCH_WINDOW: u64 = 100_000;

pub(super) fn search_files_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: &str,
    offset: u64,
    limit: u32,
) -> Result<SearchResultPageDto, SearchError> {
    let start = std::time::Instant::now();
    let search_limit = search_window(offset, limit)?;
    let mut total = 0u64;
    let mut hits = Vec::new();

    for (source, storage) in crate::source_db::ready_data_sources(case_conn, case_id)? {
        let index_dir = storage
            .index_rel_path
            .as_deref()
            .map(|rel| safe_case_relative_path(case_root, rel))
            .transpose()?
            .unwrap_or_else(|| source_index_dir(case_root, &source.id));
        if !index_dir.exists() {
            continue;
        }
        let index_dir = safe_existing_case_path(case_root, &index_dir)?;
        let index = SearchIndex::open(&index_dir).map_err(|e| SearchError::Index(e.to_string()))?;
        let SearchResult {
            hits: source_hits,
            total_count,
        } = index
            .search(query, search_limit)
            .map_err(|e| SearchError::Index(e.to_string()))?;
        total = total.saturating_add(total_count);
        hits.extend(
            search_hits_to_dto(source_hits)
                .into_iter()
                .map(|hit| source_scoped_search_hit(hit, &source.id)),
        );
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file_id.cmp(&b.file_id))
    });

    Ok(SearchResultPageDto {
        total,
        took_ms: start.elapsed().as_millis() as u64,
        items: hits
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect(),
    })
}

fn search_window(offset: u64, limit: u32) -> Result<usize, SearchError> {
    let search_limit = offset.saturating_add(limit as u64);
    if search_limit > MAX_CASE_SEARCH_WINDOW {
        return Err(SearchError::InvalidInput(format!(
            "search window exceeds {MAX_CASE_SEARCH_WINDOW} results"
        )));
    }
    usize::try_from(search_limit)
        .map_err(|_| SearchError::InvalidInput("search offset is too large".to_string()))
}

fn search_hits_to_dto(hits: Vec<SearchHit>) -> Vec<SearchHitDto> {
    hits.into_iter()
        .map(|hit| {
            let snippets = hit
                .snippets
                .into_iter()
                .map(|snippet| SearchSnippetDto {
                    text: snippet.text,
                    highlights: snippet
                        .highlights
                        .into_iter()
                        .map(|highlight| SearchHighlightDto {
                            start: highlight.start,
                            end: highlight.end,
                        })
                        .collect(),
                })
                .collect::<Vec<_>>();
            SearchHitDto {
                file_id: hit.file_id,
                path: hit.path,
                score: hit.score,
                snippets: if snippets.is_empty() {
                    vec![SearchSnippetDto {
                        text: String::new(),
                        highlights: vec![],
                    }]
                } else {
                    snippets
                },
            }
        })
        .collect()
}

fn source_scoped_search_hit(mut hit: SearchHitDto, data_source_id: &DataSourceId) -> SearchHitDto {
    if !hit.file_id.starts_with("ds:") {
        hit.file_id = encode_source_scoped_id(data_source_id, &hit.file_id);
    }
    hit
}
