use std::collections::VecDeque;
use std::path::Path;

use domain::DataSourceId;
use rusqlite::Connection;
use search::{
    FileSearchAfterKey, FileSearchQuerySession, FileSearchRankedHit, FileSearchSortDirection,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use transport::commands::SearchFilesRequest;
use transport::dto::SearchFileResultPageDto;
use transport::paging::{decode_opaque_cursor, encode_opaque_cursor};

use super::{
    current_sources, file_hit_to_dto, file_search_options, search_rank_order, SearchSource,
    MAX_CASE_SEARCH_WINDOW,
};
use crate::search_service::SearchError;
use crate::source_db::encode_source_scoped_id;

const CURSOR_KIND: &str = "case-file-search-v3";
const MAX_CURSOR_SOURCES: usize = 128;
const MAX_CURSOR_FILE_ID_BYTES: usize = 2048;
const MAX_CURSOR_SORT_VALUE_BYTES: usize = 4096;
const MAX_INDEX_GENERATION_BYTES: usize = 64;

pub(super) fn search_files_for_case_cursor(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<SearchFileResultPageDto, SearchError> {
    let start = std::time::Instant::now();
    let decoded = request
        .cursor
        .as_deref()
        .map(decode_cursor)
        .transpose()?
        .map(|payload| validate_cursor(payload, case_id, request))
        .transpose()?;
    let source_set = current_sources(case_conn, case_root, case_id, request)?;
    validate_source_set(decoded.as_ref(), &source_set.sources)?;
    let options = file_search_options(request);
    let fetch_limit = request.limit as usize;
    let mut sessions =
        open_source_sessions(source_set.sources, &options, decoded.as_ref(), fetch_limit)?;
    let total = sessions
        .iter()
        .map(|source| source.state.total_count)
        .sum::<u64>();
    let available = total.min(MAX_CASE_SEARCH_WINDOW);
    let consumed = decoded.as_ref().map_or(0, |payload| payload.consumed);
    validate_window(decoded.as_ref(), total, available, consumed)?;
    let mut items = Vec::with_capacity(fetch_limit);
    while items.len() < fetch_limit && consumed.saturating_add(items.len() as u64) < available {
        let Some(source_index) = next_source(&sessions, options.sort_direction) else {
            return Err(SearchError::Index(
                "search cursor exhausted before reaching its stable result count".to_string(),
            ));
        };
        items.push(sessions[source_index].pop_materialized()?);
    }
    let next_consumed = consumed.saturating_add(items.len() as u64);
    let next_cursor = if next_consumed < available {
        Some(encode_cursor(SearchCursorPayload {
            kind: CURSOR_KIND.to_string(),
            case_id: case_id.0.clone(),
            request_hash: request_hash(request),
            consumed: next_consumed,
            total,
            available,
            sources: sessions.into_iter().map(|source| source.state).collect(),
        })?)
    } else {
        None
    };
    Ok(SearchFileResultPageDto {
        total,
        available,
        truncated: available < total,
        took_ms: start.elapsed().as_millis() as u64,
        items,
        coverage: source_set.coverage,
        next_cursor,
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchCursorPayload {
    kind: String,
    case_id: String,
    request_hash: String,
    consumed: u64,
    total: u64,
    available: u64,
    sources: Vec<SearchSourceCursor>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchSourceCursor {
    data_source_id: String,
    index_generation: String,
    schema_version: u32,
    snapshot_opstamp: u64,
    total_count: u64,
    after: Option<FileSearchAfterKey>,
}

struct SourceCursorSession {
    data_source_id: DataSourceId,
    data_source_name: String,
    session: FileSearchQuerySession,
    state: SearchSourceCursor,
    hits: VecDeque<RankedSearchHit>,
}

struct RankedSearchHit {
    scoped_file_id: String,
    ranked: FileSearchRankedHit,
}

fn open_source_sessions(
    sources: Vec<SearchSource>,
    options: &search::FileSearchOptions,
    cursor: Option<&SearchCursorPayload>,
    limit: usize,
) -> Result<Vec<SourceCursorSession>, SearchError> {
    sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            let expected = cursor.map(|payload| &payload.sources[index]);
            let session = source
                .index
                .file_query_session(options)
                .map_err(index_error)?;
            if expected.is_some_and(|state| {
                state.index_generation != session.index_generation()
                    || state.schema_version != session.schema_version()
                    || state.snapshot_opstamp != session.snapshot_opstamp()
            }) {
                return Err(stale_cursor("a source file index changed"));
            }
            let page = session
                .rank_after(expected.and_then(|state| state.after.as_ref()), limit)
                .map_err(index_error)?;
            if expected.is_some_and(|state| state.total_count != page.total_count) {
                return Err(stale_cursor("a source result count changed"));
            }
            let state = SearchSourceCursor {
                data_source_id: source.data_source_id.0.clone(),
                index_generation: session.index_generation().to_string(),
                schema_version: session.schema_version(),
                snapshot_opstamp: session.snapshot_opstamp(),
                total_count: page.total_count,
                after: expected.and_then(|state| state.after.clone()),
            };
            let hits = page
                .hits
                .into_iter()
                .map(|ranked| RankedSearchHit {
                    scoped_file_id: encode_source_scoped_id(
                        &source.data_source_id,
                        ranked.file_id(),
                    ),
                    ranked,
                })
                .collect();
            Ok(SourceCursorSession {
                data_source_id: source.data_source_id,
                data_source_name: source.data_source_name,
                session,
                state,
                hits,
            })
        })
        .collect()
}

impl SourceCursorSession {
    fn front_key(&self) -> Option<(&str, &str)> {
        self.hits
            .front()
            .map(|hit| (hit.ranked.sort_value(), hit.scoped_file_id.as_str()))
    }

    fn pop_materialized(&mut self) -> Result<transport::dto::SearchFileHitDto, SearchError> {
        let ranked = self.hits.pop_front().ok_or_else(|| {
            SearchError::Index("search merge selected an empty source cursor".to_string())
        })?;
        self.state.after = Some(ranked.ranked.after_key());
        let hit = self
            .session
            .materialize(ranked.ranked)
            .map_err(index_error)?;
        Ok(file_hit_to_dto(
            hit,
            &self.data_source_id,
            &self.data_source_name,
        ))
    }
}

fn next_source(
    sources: &[SourceCursorSession],
    direction: FileSearchSortDirection,
) -> Option<usize> {
    let mut selected: Option<usize> = None;
    for (index, source) in sources.iter().enumerate() {
        let Some((sort_value, file_id)) = source.front_key() else {
            continue;
        };
        let precedes = selected
            .and_then(|current| sources[current].front_key())
            .is_none_or(|(current_sort, current_file_id)| {
                search_rank_order(
                    sort_value,
                    file_id,
                    current_sort,
                    current_file_id,
                    direction,
                )
                .is_lt()
            });
        if precedes {
            selected = Some(index);
        }
    }
    selected
}

fn validate_cursor(
    payload: SearchCursorPayload,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<SearchCursorPayload, SearchError> {
    if payload.kind != CURSOR_KIND
        || payload.case_id != case_id.0
        || payload.request_hash != request_hash(request)
    {
        return Err(stale_cursor(
            "the cursor does not match this search request",
        ));
    }
    if payload.sources.len() > MAX_CURSOR_SOURCES
        || payload.sources.iter().any(|source| {
            source.data_source_id.is_empty()
                || source.index_generation.is_empty()
                || source.index_generation.len() > MAX_INDEX_GENERATION_BYTES
                || source.schema_version == 0
                || source.after.as_ref().is_some_and(|after| {
                    after.file_id.is_empty()
                        || after.file_id.len() > MAX_CURSOR_FILE_ID_BYTES
                        || after.sort_value.len() > MAX_CURSOR_SORT_VALUE_BYTES
                })
        })
    {
        return Err(SearchError::InvalidInput(
            "search cursor contains invalid bounded state".to_string(),
        ));
    }
    Ok(payload)
}

fn validate_source_set(
    cursor: Option<&SearchCursorPayload>,
    sources: &[SearchSource],
) -> Result<(), SearchError> {
    let Some(cursor) = cursor else {
        return Ok(());
    };
    let current_ids = sources
        .iter()
        .map(|source| source.data_source_id.0.as_str())
        .collect::<Vec<_>>();
    let cursor_ids = cursor
        .sources
        .iter()
        .map(|source| source.data_source_id.as_str())
        .collect::<Vec<_>>();
    if current_ids != cursor_ids {
        return Err(stale_cursor("the set of searchable data sources changed"));
    }
    Ok(())
}

fn validate_window(
    cursor: Option<&SearchCursorPayload>,
    total: u64,
    available: u64,
    consumed: u64,
) -> Result<(), SearchError> {
    if consumed > available
        || cursor.is_some_and(|payload| payload.total != total || payload.available != available)
    {
        return Err(stale_cursor("the search result window changed"));
    }
    Ok(())
}

fn request_hash(request: &SearchFilesRequest) -> String {
    let payload = serde_json::json!({
        "query": request.query,
        "matchPath": request.match_path,
        "entryType": request.entry_type,
        "extensions": request.extensions,
        "dataSourceIds": request.data_source_ids,
        "sortKey": request.sort_key,
        "sortDirection": request.sort_direction,
    });
    hex::encode(Sha256::digest(payload.to_string().as_bytes()))
}

fn decode_cursor(cursor: &str) -> Result<SearchCursorPayload, SearchError> {
    decode_opaque_cursor(cursor)
        .map_err(|error| SearchError::InvalidInput(format!("invalid search cursor: {error}")))
}

fn encode_cursor(payload: SearchCursorPayload) -> Result<String, SearchError> {
    encode_opaque_cursor(&payload)
        .map_err(|error| SearchError::Other(format!("failed to encode search cursor: {error}")))
}

fn index_error(error: search::indexer::tantivy_writer::IndexError) -> SearchError {
    SearchError::Index(error.to_string())
}

fn stale_cursor(reason: &str) -> SearchError {
    SearchError::InvalidInput(format!("stale search cursor: {reason}; restart the search"))
}
