use std::collections::{HashSet, VecDeque};
use std::path::Path;

use domain::DataSourceId;
use rusqlite::Connection;
use search::{
    FileEntryTypeFilter, FileSearchHit, FileSearchOptions, FileSearchQuerySession,
    FileSearchRankedHit, FileSearchSortDirection, FileSearchSortField, SearchIndex,
};
use transport::commands::{
    FileSortDirectionDto, SearchEntryTypeDto, SearchFilesRequest, SearchSortKeyDto,
};
use transport::dto::{SearchCoverageDto, SearchFileHitDto, SearchFileResultPageDto};

use super::SearchError;
use crate::source_db::{
    encode_source_scoped_id, open_ready_source_read_only_by_id, registered_source_index_dir,
    safe_existing_case_path,
};

mod cursor;

pub(super) const MAX_CASE_SEARCH_WINDOW: u64 = 100_000;

pub(super) fn search_files_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<SearchFileResultPageDto, SearchError> {
    let start = std::time::Instant::now();
    let source_set = current_sources(case_conn, case_root, case_id, request)?;
    let options = file_search_options(request);
    let scan_end = bounded_scan_end(request.offset, request.limit);
    let mut total = 0u64;
    let mut sources = Vec::new();
    for source in source_set.sources {
        let cursor = SourceSearchCursor::load(source, &options, scan_end)?;
        total = total.saturating_add(cursor.total_count);
        sources.push(cursor);
    }
    let available = total.min(MAX_CASE_SEARCH_WINDOW);
    let mut items = Vec::with_capacity(request.limit as usize);
    if request.limit > 0 && request.offset < available {
        let merge_end = (scan_end as u64).min(available);
        for position in 0..merge_end {
            let Some(source_index) = next_search_source(&sources, options.sort_direction) else {
                break;
            };
            if position >= request.offset {
                items.push(sources[source_index].pop_materialized()?);
            } else {
                sources[source_index].discard_front()?;
            }
        }
    }
    Ok(SearchFileResultPageDto {
        total,
        available,
        truncated: available < total,
        took_ms: start.elapsed().as_millis() as u64,
        items,
        coverage: source_set.coverage,
        next_cursor: None,
    })
}

pub(super) fn search_files_for_case_cursor(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<SearchFileResultPageDto, SearchError> {
    cursor::search_files_for_case_cursor(case_conn, case_root, case_id, request)
}

pub(super) struct SearchSource {
    pub(super) data_source_id: DataSourceId,
    pub(super) data_source_name: String,
    pub(super) index: SearchIndex,
}

pub(super) struct SearchSourceSet {
    pub(super) sources: Vec<SearchSource>,
    pub(super) coverage: SearchCoverageDto,
}

struct SourceSearchCursor {
    data_source_id: DataSourceId,
    data_source_name: String,
    total_count: u64,
    session: FileSearchQuerySession,
    hits: VecDeque<RankedSearchHit>,
}

impl SourceSearchCursor {
    fn load(
        source: SearchSource,
        options: &FileSearchOptions,
        scan_end: usize,
    ) -> Result<Self, SearchError> {
        let session = source
            .index
            .file_query_session(options)
            .map_err(index_error)?;
        let page = session.rank_after(None, scan_end).map_err(index_error)?;
        let hits = page
            .hits
            .into_iter()
            .map(|ranked| RankedSearchHit {
                scoped_file_id: encode_source_scoped_id(&source.data_source_id, ranked.file_id()),
                ranked,
            })
            .collect();
        Ok(Self {
            data_source_id: source.data_source_id,
            data_source_name: source.data_source_name,
            total_count: page.total_count,
            session,
            hits,
        })
    }

    fn front_key(&self) -> Option<(&str, &str)> {
        self.hits
            .front()
            .map(|hit| (hit.ranked.sort_value(), hit.scoped_file_id.as_str()))
    }

    fn discard_front(&mut self) -> Result<(), SearchError> {
        self.hits
            .pop_front()
            .map(|_| ())
            .ok_or_else(empty_cursor_error)
    }

    fn pop_materialized(&mut self) -> Result<SearchFileHitDto, SearchError> {
        let ranked = self.hits.pop_front().ok_or_else(empty_cursor_error)?;
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

struct RankedSearchHit {
    scoped_file_id: String,
    ranked: FileSearchRankedHit,
}

pub(super) fn current_sources(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<SearchSourceSet, SearchError> {
    let selected = request
        .data_source_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut sources = Vec::new();
    let mut coverage = SearchCoverageDto::default();
    for (source, _) in crate::source_db::ready_data_sources(case_conn, case_id)? {
        if !selected.is_empty() && !selected.contains(source.id.0.as_str()) {
            continue;
        }
        coverage.ready_source_count = coverage.ready_source_count.saturating_add(1);
        let ready = open_ready_source_read_only_by_id(case_conn, case_root, case_id, &source.id)?;
        let expected = count_source_entries(&ready.connection)?;
        coverage.expected_entry_count = coverage.expected_entry_count.saturating_add(expected);
        let index_dir = registered_source_index_dir(case_conn, case_root, &source.id)?;
        if !index_dir.exists() {
            coverage.missing_source_ids.push(source.id.0.clone());
            continue;
        }
        let safe_index_dir = safe_existing_case_path(case_root, &index_dir)?;
        let index = match SearchIndex::open(&safe_index_dir).and_then(|index| {
            index.validate_file_search_schema()?;
            Ok(index)
        }) {
            Ok(index) => index,
            Err(error) => {
                tracing::warn!(
                    data_source_id = %source.id.0,
                    error = %error,
                    "File search index is unavailable; re-run data-source analysis to rebuild the index"
                );
                coverage.missing_source_ids.push(source.id.0.clone());
                continue;
            }
        };
        let indexed = index.document_count().map_err(index_error)?;
        coverage.indexed_entry_count = coverage.indexed_entry_count.saturating_add(indexed);
        coverage.indexed_source_count = coverage.indexed_source_count.saturating_add(1);
        if indexed != expected {
            coverage.missing_source_ids.push(source.id.0.clone());
        }
        sources.push(SearchSource {
            data_source_id: source.id,
            data_source_name: source.name,
            index,
        });
    }
    coverage.missing_source_ids.sort_unstable();
    coverage.missing_source_ids.dedup();
    coverage.complete = coverage.ready_source_count == coverage.indexed_source_count
        && coverage.expected_entry_count == coverage.indexed_entry_count
        && coverage.missing_source_ids.is_empty();
    sources.sort_unstable_by(|left, right| left.data_source_id.0.cmp(&right.data_source_id.0));
    Ok(SearchSourceSet { sources, coverage })
}

pub(super) fn file_search_options(request: &SearchFilesRequest) -> FileSearchOptions {
    FileSearchOptions {
        query: request.query.clone(),
        match_path: request.match_path,
        entry_type: match request.entry_type {
            SearchEntryTypeDto::Any => FileEntryTypeFilter::Any,
            SearchEntryTypeDto::File => FileEntryTypeFilter::File,
            SearchEntryTypeDto::Directory => FileEntryTypeFilter::Directory,
        },
        extensions: request.extensions.clone(),
        sort_field: match request.sort_key {
            SearchSortKeyDto::Name => FileSearchSortField::Name,
            SearchSortKeyDto::Path => FileSearchSortField::Path,
            SearchSortKeyDto::Size => FileSearchSortField::Size,
            SearchSortKeyDto::ModifiedAt => FileSearchSortField::ModifiedAt,
        },
        sort_direction: sort_direction(request.sort_direction),
    }
}

pub(super) fn sort_direction(direction: FileSortDirectionDto) -> FileSearchSortDirection {
    match direction {
        FileSortDirectionDto::Asc => FileSearchSortDirection::Asc,
        FileSortDirectionDto::Desc => FileSearchSortDirection::Desc,
    }
}

pub(super) fn search_rank_order(
    left_sort: &str,
    left_file_id: &str,
    right_sort: &str,
    right_file_id: &str,
    direction: FileSearchSortDirection,
) -> std::cmp::Ordering {
    let primary = match direction {
        FileSearchSortDirection::Asc => left_sort.cmp(right_sort),
        FileSearchSortDirection::Desc => right_sort.cmp(left_sort),
    };
    primary.then_with(|| left_file_id.cmp(right_file_id))
}

pub(super) fn file_hit_to_dto(
    hit: FileSearchHit,
    data_source_id: &DataSourceId,
    data_source_name: &str,
) -> SearchFileHitDto {
    SearchFileHitDto {
        file_id: encode_source_scoped_id(data_source_id, &hit.file_id),
        data_source_id: data_source_id.0.clone(),
        data_source_name: data_source_name.to_string(),
        name: hit.name,
        path: hit.path,
        entry_type: hit.entry_type,
        extension: (!hit.extension.is_empty()).then_some(hit.extension),
        size: hit.size,
        modified_at: hit.modified_at.and_then(|timestamp| {
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp)
                .map(|value| value.to_rfc3339())
        }),
        deleted: hit.deleted,
        hidden: hit.hidden,
        system: hit.system,
        encrypted: hit.encrypted,
    }
}

pub(super) fn bounded_scan_end(offset: u64, limit: u32) -> usize {
    if limit == 0 || offset >= MAX_CASE_SEARCH_WINDOW {
        return 0;
    }
    offset
        .saturating_add(u64::from(limit))
        .min(MAX_CASE_SEARCH_WINDOW) as usize
}

fn next_search_source(
    sources: &[SourceSearchCursor],
    direction: FileSearchSortDirection,
) -> Option<usize> {
    let mut selected: Option<usize> = None;
    for (index, source) in sources.iter().enumerate() {
        let Some((sort_value, file_id)) = source.front_key() else {
            continue;
        };
        let precedes = selected
            .and_then(|current| sources[current].front_key())
            .is_none_or(|(current_sort, current_id)| {
                search_rank_order(sort_value, file_id, current_sort, current_id, direction).is_lt()
            });
        if precedes {
            selected = Some(index);
        }
    }
    selected
}

fn count_source_entries(connection: &Connection) -> Result<u64, SearchError> {
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .map_err(|error| SearchError::Other(format!("count searchable file entries: {error}")))?;
    Ok(count.max(0) as u64)
}

fn index_error(error: search::indexer::tantivy_writer::IndexError) -> SearchError {
    SearchError::Index(error.to_string())
}

fn empty_cursor_error() -> SearchError {
    SearchError::Index("search merge selected a source without a buffered hit".to_string())
}
