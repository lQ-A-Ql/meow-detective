use std::collections::VecDeque;
use std::path::Path;

use domain::DataSourceId;
use rusqlite::Connection;
use search::{SearchAfterKey, SearchIndex, SearchQuerySession, SearchRankedHit};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use transport::dto::SearchResultPageDto;
use transport::paging::{decode_opaque_cursor, encode_opaque_cursor};

use super::{
    search_hit_to_dto, search_rank_order, source_scoped_search_hit, MAX_CASE_SEARCH_WINDOW,
};
use crate::search_service::SearchError;
use crate::source_db::{
    encode_source_scoped_id, registered_source_index_dir, safe_existing_case_path,
};

const CURSOR_KIND: &str = "case-search-v2";
const MAX_CURSOR_SOURCES: usize = 512;
const MAX_CURSOR_FILE_ID_BYTES: usize = 2048;
const MAX_INDEX_GENERATION_BYTES: usize = 64;

pub(super) fn search_files_for_case_cursor(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: &str,
    cursor: Option<&str>,
    limit: u32,
) -> Result<SearchResultPageDto, SearchError> {
    let start = std::time::Instant::now();
    let decoded = cursor
        .map(decode_cursor)
        .transpose()?
        .map(|payload| validate_cursor(payload, case_id, query))
        .transpose()?;
    let sources = current_sources(case_conn, case_root, case_id)?;
    validate_source_set(decoded.as_ref(), &sources)?;
    let fetch_limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let mut sessions = open_source_sessions(sources, query, decoded.as_ref(), fetch_limit)?;
    let total = sessions
        .iter()
        .map(|source| source.state.total_count)
        .sum::<u64>();
    let available = total.min(MAX_CASE_SEARCH_WINDOW);
    let consumed = decoded.as_ref().map_or(0, |payload| payload.consumed);
    validate_window(decoded.as_ref(), total, available, consumed)?;

    let mut items = Vec::with_capacity(fetch_limit);
    while items.len() < fetch_limit && consumed.saturating_add(items.len() as u64) < available {
        let Some(source_index) = next_source(&sessions) else {
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
            query_hash: query_hash(query),
            consumed: next_consumed,
            total,
            available,
            sources: sessions.into_iter().map(|source| source.state).collect(),
        })?)
    } else {
        None
    };

    Ok(SearchResultPageDto {
        total,
        available,
        truncated: available < total,
        took_ms: start.elapsed().as_millis() as u64,
        items,
        next_cursor,
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchCursorPayload {
    kind: String,
    case_id: String,
    query_hash: String,
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
    after: Option<SearchAfterKey>,
}

struct SourceCursorSession {
    data_source_id: DataSourceId,
    session: SearchQuerySession,
    state: SearchSourceCursor,
    hits: VecDeque<RankedSearchHit>,
}

struct RankedSearchHit {
    scoped_file_id: String,
    ranked: SearchRankedHit,
}

fn current_sources(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
) -> Result<Vec<(DataSourceId, SearchIndex)>, SearchError> {
    let mut sources = Vec::new();
    for (source, _) in crate::source_db::ready_data_sources(case_conn, case_id)? {
        let index_dir = registered_source_index_dir(case_conn, case_root, &source.id)?;
        if !index_dir.exists() {
            continue;
        }
        let safe_index_dir = safe_existing_case_path(case_root, &index_dir)?;
        let index = SearchIndex::open(&safe_index_dir)
            .map_err(|error| SearchError::Index(error.to_string()))?;
        if !index.supports_stable_paging() {
            return Err(SearchError::Unsupported(
                "Search index schema does not support deterministic pagination; rebuild the data source search index"
                    .to_string(),
            ));
        }
        sources.push((source.id, index));
    }
    sources.sort_unstable_by(|left, right| left.0 .0.cmp(&right.0 .0));
    Ok(sources)
}

fn open_source_sessions(
    sources: Vec<(DataSourceId, SearchIndex)>,
    query: &str,
    cursor: Option<&SearchCursorPayload>,
    limit: usize,
) -> Result<Vec<SourceCursorSession>, SearchError> {
    sources
        .into_iter()
        .enumerate()
        .map(|(index, (data_source_id, search_index))| {
            let expected = cursor.map(|payload| &payload.sources[index]);
            let session = search_index
                .query_session(query)
                .map_err(|error| SearchError::Index(error.to_string()))?;
            if expected.is_some_and(|state| {
                state.index_generation != session.index_generation()
                    || state.schema_version != session.schema_version()
            }) {
                return Err(stale_cursor("a source search index generation changed"));
            }
            if expected.is_some_and(|state| state.snapshot_opstamp != session.snapshot_opstamp()) {
                return Err(stale_cursor("a source search index changed"));
            }
            let page = session
                .rank_after(expected.and_then(|state| state.after.as_ref()), limit)
                .map_err(|error| SearchError::Index(error.to_string()))?;
            if expected.is_some_and(|state| state.total_count != page.total_count) {
                return Err(stale_cursor("a source result count changed"));
            }
            let state = SearchSourceCursor {
                data_source_id: data_source_id.0.clone(),
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
                    scoped_file_id: encode_source_scoped_id(&data_source_id, ranked.file_id()),
                    ranked,
                })
                .collect();
            Ok(SourceCursorSession {
                data_source_id,
                session,
                state,
                hits,
            })
        })
        .collect()
}

impl SourceCursorSession {
    fn front_key(&self) -> Option<(f64, &str)> {
        self.hits
            .front()
            .map(|hit| (hit.ranked.score(), hit.scoped_file_id.as_str()))
    }

    fn pop_materialized(&mut self) -> Result<transport::dto::SearchHitDto, SearchError> {
        let ranked = self.hits.pop_front().ok_or_else(|| {
            SearchError::Index("search merge selected an empty source cursor".to_string())
        })?;
        self.state.after = Some(ranked.ranked.after_key());
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

fn next_source(sources: &[SourceCursorSession]) -> Option<usize> {
    let mut selected: Option<usize> = None;
    for (index, source) in sources.iter().enumerate() {
        let Some((score, file_id)) = source.front_key() else {
            continue;
        };
        let precedes_selected = selected
            .and_then(|current| sources[current].front_key())
            .is_none_or(|(current_score, current_file_id)| {
                search_rank_order(score, file_id, current_score, current_file_id).is_lt()
            });
        if precedes_selected {
            selected = Some(index);
        }
    }
    selected
}

fn validate_cursor(
    payload: SearchCursorPayload,
    case_id: &domain::CaseId,
    query: &str,
) -> Result<SearchCursorPayload, SearchError> {
    if payload.kind != CURSOR_KIND
        || payload.case_id != case_id.0
        || payload.query_hash != query_hash(query)
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
                        || !f32::from_bits(after.score_bits).is_finite()
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
    sources: &[(DataSourceId, SearchIndex)],
) -> Result<(), SearchError> {
    let Some(cursor) = cursor else {
        return Ok(());
    };
    let current_ids = sources
        .iter()
        .map(|(source_id, _)| source_id.0.as_str())
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

fn decode_cursor(cursor: &str) -> Result<SearchCursorPayload, SearchError> {
    decode_opaque_cursor(cursor)
        .map_err(|error| SearchError::InvalidInput(format!("invalid search cursor: {error}")))
}

fn encode_cursor(payload: SearchCursorPayload) -> Result<String, SearchError> {
    encode_opaque_cursor(&payload)
        .map_err(|error| SearchError::Other(format!("failed to encode search cursor: {error}")))
}

fn query_hash(query: &str) -> String {
    hex::encode(Sha256::digest(query.as_bytes()))
}

fn stale_cursor(reason: &str) -> SearchError {
    SearchError::InvalidInput(format!("stale search cursor: {reason}; restart the search"))
}
