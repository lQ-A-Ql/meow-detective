use std::path::Path;
use std::sync::Arc;

use domain::DataSourceId;
use rusqlite::Connection;
use search::{
    FileEntryTypeFilter, FileSearchHit, FileSearchOptions, FileSearchSortDirection,
    FileSearchSortField,
};
use transport::commands::{
    FileSortDirectionDto, SearchEntryTypeDto, SearchFilesRequest, SearchSortKeyDto,
};
use transport::dto::{SearchCoverageDto, SearchFileHitDto, SearchFileResultPageDto};

use super::SearchError;
use crate::source_db::encode_source_scoped_id;

mod buffer;
mod cursor;
mod merge;
mod offset;
pub(super) mod pool;
mod source;
use source::current_sources;

pub(super) const MAX_OFFSET_WINDOW: u64 = 100_000;

pub(super) fn search_files_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<SearchFileResultPageDto, SearchError> {
    offset::query(case_conn, case_root, case_id, request)
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
    pub(super) session: Arc<pool::CachedSession>,
}

pub(super) struct SearchSourceSet {
    pub(super) sources: Vec<SearchSource>,
    pub(super) coverage: SearchCoverageDto,
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
        match_source: hit.match_source,
        matched_sources: hit.matched_sources,
        snippet: hit.snippet,
    }
}

pub(super) fn bounded_scan_end(offset: u64, limit: u32) -> usize {
    if limit == 0 || offset >= MAX_OFFSET_WINDOW {
        return 0;
    }
    offset
        .saturating_add(u64::from(limit))
        .min(MAX_OFFSET_WINDOW) as usize
}

fn index_error(error: search::indexer::tantivy_writer::IndexError) -> SearchError {
    SearchError::Index(error.to_string())
}
