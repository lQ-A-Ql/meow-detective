use std::path::Path;

use rusqlite::Connection;
use search::FileSearchAfterKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use transport::commands::SearchFilesRequest;
use transport::dto::SearchFileResultPageDto;
use transport::paging::{decode_opaque_cursor, encode_opaque_cursor};

use super::{
    buffer::SourceBuffer, current_sources, file_search_options, merge::MergeHeap, SearchSource,
};
use crate::search_service::SearchError;

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
    let mut sessions = open_source_sessions(source_set.sources, decoded.as_ref(), fetch_limit)?;
    let total = sessions
        .iter()
        .map(|source| source.state.total_count)
        .fold(0u64, u64::saturating_add);
    let available = total;
    let consumed = decoded.as_ref().map_or(0, |payload| payload.consumed);
    validate_window(decoded.as_ref(), total, available, consumed)?;
    let mut heap = MergeHeap::new(options.sort_direction);
    for (index, source) in sessions.iter().enumerate() {
        if let Some((sort, id)) = source.front_key() {
            heap.push(index, sort, id);
        }
    }
    let mut items = Vec::with_capacity(fetch_limit);
    while items.len() < fetch_limit && consumed.saturating_add(items.len() as u64) < available {
        let Some(source_index) = heap.pop() else {
            return Err(SearchError::Index(
                "search cursor exhausted before reaching its stable result count".to_string(),
            ));
        };
        items.push(sessions[source_index].pop_materialized()?);
        if let Some((sort, id)) = sessions[source_index].front_key() {
            heap.push(source_index, sort, id);
        }
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
    state: SearchSourceCursor,
    buffer: SourceBuffer,
}

fn open_source_sessions(
    sources: Vec<SearchSource>,
    cursor: Option<&SearchCursorPayload>,
    limit: usize,
) -> Result<Vec<SourceCursorSession>, SearchError> {
    sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            let expected = cursor.map(|payload| &payload.sources[index]);
            let session = &source.session;
            if expected.is_some_and(|state| {
                state.index_generation != session.index_generation()
                    || state.schema_version != session.schema_version()
                    || state.snapshot_opstamp != session.snapshot_opstamp()
            }) {
                return Err(stale_cursor("a source file index changed"));
            }
            let mut state = SearchSourceCursor {
                data_source_id: source.data_source_id.0.clone(),
                index_generation: session.index_generation().to_string(),
                schema_version: session.schema_version(),
                snapshot_opstamp: session.snapshot_opstamp(),
                total_count: 0,
                after: expected.and_then(|state| state.after.clone()),
            };
            let buffer = SourceBuffer::new(
                source,
                state.after.clone(),
                expected.map(|state| state.total_count),
                limit,
            )?;
            if expected.is_some_and(|state| state.total_count != buffer.total) {
                return Err(stale_cursor("a source result count changed"));
            }
            state.total_count = buffer.total;
            Ok(SourceCursorSession { state, buffer })
        })
        .collect()
}

impl SourceCursorSession {
    fn front_key(&self) -> Option<(&str, &str)> {
        self.buffer.front_key()
    }

    fn pop_materialized(&mut self) -> Result<transport::dto::SearchFileHitDto, SearchError> {
        let hit = self.buffer.take(true)?.ok_or_else(|| {
            SearchError::Index("search merge selected an empty source cursor".to_owned())
        })?;
        self.state.after = self.buffer.after.clone();
        Ok(hit)
    }
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

fn stale_cursor(reason: &str) -> SearchError {
    SearchError::InvalidInput(format!("stale search cursor: {reason}; restart the search"))
}

#[cfg(test)]
#[path = "../../../tests/unit/search_service/case_search_cursor.rs"]
mod tests;
