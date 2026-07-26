use domain::DataSourceId;
use rusqlite::Connection;
use search::{SearchHit, SearchIndex, SearchQuerySession, SearchRankedHit};
use std::collections::VecDeque;
use std::path::Path;
use transport::dto::{SearchHighlightDto, SearchHitDto, SearchResultPageDto, SearchSnippetDto};

use super::SearchError;
use crate::source_db::{
    encode_source_scoped_id, registered_source_index_dir, safe_existing_case_path,
};

mod cursor;

pub(super) const MAX_CASE_SEARCH_WINDOW: u64 = 100_000;

pub(super) fn search_files_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: &str,
    offset: u64,
    limit: u32,
) -> Result<SearchResultPageDto, SearchError> {
    let start = std::time::Instant::now();
    let scan_end = bounded_scan_end(offset, limit);
    let mut total = 0u64;
    let mut sources = Vec::new();

    for (source, _) in crate::source_db::ready_data_sources(case_conn, case_id)? {
        let index_dir = registered_source_index_dir(case_conn, case_root, &source.id)?;
        if !index_dir.exists() {
            continue;
        }
        let index_dir = safe_existing_case_path(case_root, &index_dir)?;
        let index = SearchIndex::open(&index_dir).map_err(|e| SearchError::Index(e.to_string()))?;
        let cursor = SourceSearchCursor::load(source.id, index, query, scan_end)?;
        total = total.saturating_add(cursor.total_count);
        sources.push(cursor);
    }

    let available = total.min(MAX_CASE_SEARCH_WINDOW);
    let mut items = Vec::with_capacity(limit as usize);
    if limit > 0 && offset < available {
        let mut position = 0u64;
        let merge_end = (scan_end as u64).min(available);
        while position < merge_end {
            let Some(source_index) = next_search_source(&sources) else {
                break;
            };
            if position >= offset {
                items.push(sources[source_index].pop_materialized()?);
            } else {
                sources[source_index].discard_front()?;
            }
            position = position.saturating_add(1);
        }
    }

    Ok(SearchResultPageDto {
        total,
        available,
        truncated: available < total,
        took_ms: start.elapsed().as_millis() as u64,
        items,
        next_cursor: None,
    })
}

pub(super) fn search_files_for_case_cursor(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: &str,
    cursor: Option<&str>,
    limit: u32,
) -> Result<SearchResultPageDto, SearchError> {
    cursor::search_files_for_case_cursor(case_conn, case_root, case_id, query, cursor, limit)
}

struct SourceSearchCursor {
    data_source_id: DataSourceId,
    total_count: u64,
    session: SearchQuerySession,
    hits: VecDeque<RankedSearchHit>,
}

impl SourceSearchCursor {
    fn load(
        data_source_id: DataSourceId,
        index: SearchIndex,
        query: &str,
        scan_end: usize,
    ) -> Result<Self, SearchError> {
        if !index.supports_stable_paging() {
            return Err(SearchError::Unsupported(
                "Search index schema does not support deterministic pagination; rebuild the data source search index"
                    .to_string(),
            ));
        }
        let session = index
            .query_session(query)
            .map_err(|error| SearchError::Index(error.to_string()))?;
        let page = session
            .rank_page(0, scan_end)
            .map_err(|error| SearchError::Index(error.to_string()))?;
        let hits = page
            .hits
            .into_iter()
            .map(|ranked| RankedSearchHit {
                scoped_file_id: encode_source_scoped_id(&data_source_id, ranked.file_id()),
                ranked,
            })
            .collect();
        Ok(Self {
            data_source_id,
            total_count: page.total_count,
            session,
            hits,
        })
    }

    fn front_key(&self) -> Option<(f64, &str)> {
        self.hits
            .front()
            .map(|hit| (hit.ranked.score(), hit.scoped_file_id.as_str()))
    }

    fn discard_front(&mut self) -> Result<(), SearchError> {
        if self.hits.pop_front().is_none() {
            return Err(empty_cursor_error());
        }
        Ok(())
    }

    fn pop_materialized(&mut self) -> Result<SearchHitDto, SearchError> {
        let ranked = self.hits.pop_front().ok_or_else(empty_cursor_error)?;
        let hit = self
            .session
            .materialize(ranked.ranked)
            .map_err(|error| SearchError::Index(error.to_string()))?;
        Ok(source_scoped_search_hit(
            search_hit_to_dto(hit),
            &self.data_source_id,
        ))
    }
}

struct RankedSearchHit {
    scoped_file_id: String,
    ranked: SearchRankedHit,
}

pub(super) fn bounded_scan_end(offset: u64, limit: u32) -> usize {
    if limit == 0 || offset >= MAX_CASE_SEARCH_WINDOW {
        return 0;
    }
    offset
        .saturating_add(u64::from(limit))
        .min(MAX_CASE_SEARCH_WINDOW) as usize
}

fn next_search_source(sources: &[SourceSearchCursor]) -> Option<usize> {
    let mut selected = None;
    for (index, source) in sources.iter().enumerate() {
        if source.front_key().is_none() {
            continue;
        }
        if selected.is_none_or(|current| search_cursor_precedes(source, &sources[current])) {
            selected = Some(index);
        }
    }
    selected
}

fn search_cursor_precedes(left: &SourceSearchCursor, right: &SourceSearchCursor) -> bool {
    let Some((left_score, left_file_id)) = left.front_key() else {
        return false;
    };
    let Some((right_score, right_file_id)) = right.front_key() else {
        return true;
    };
    search_rank_order(left_score, left_file_id, right_score, right_file_id).is_lt()
}

pub(super) fn search_rank_order(
    left_score: f64,
    left_file_id: &str,
    right_score: f64,
    right_file_id: &str,
) -> std::cmp::Ordering {
    right_score
        .total_cmp(&left_score)
        .then_with(|| left_file_id.cmp(right_file_id))
}

pub(super) fn search_hit_to_dto(hit: SearchHit) -> SearchHitDto {
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
}

pub(super) fn source_scoped_search_hit(
    mut hit: SearchHitDto,
    data_source_id: &DataSourceId,
) -> SearchHitDto {
    if !hit.file_id.starts_with("ds:") {
        hit.file_id = encode_source_scoped_id(data_source_id, &hit.file_id);
    }
    hit
}

fn empty_cursor_error() -> SearchError {
    SearchError::Index("search merge cursor selected a source without a buffered hit".to_string())
}
