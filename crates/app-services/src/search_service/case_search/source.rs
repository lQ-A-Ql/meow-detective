use super::{file_search_options, pool, SearchSource, SearchSourceSet};
use crate::search_service::SearchError;
use crate::source_db::{registered_source_index_dir, safe_existing_case_path};
use domain::{CaseId, DataSource};
use rusqlite::Connection;
use search::FileSearchOptions;
use std::{collections::HashSet, path::Path};
use transport::{commands::SearchFilesRequest, dto::SearchCoverageDto};

pub(super) fn current_sources(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &SearchFilesRequest,
) -> Result<SearchSourceSet, SearchError> {
    let canonical_root = case_root
        .canonicalize()
        .map_err(|error| SearchError::Other(error.to_string()))?;
    let ready = crate::source_db::ready_data_sources(case_conn, case_id)?;
    let ready_ids = ready
        .iter()
        .map(|(source, _)| source.id.0.as_str())
        .collect();
    pool::retain_ready_sources(&canonical_root, &case_id.0, &ready_ids)?;
    let options = file_search_options(request);
    let context = SourceQuery {
        case_conn,
        case_root,
        case_id,
        canonical_root: &canonical_root,
        options: &options,
    };
    let selected = request
        .data_source_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut sources = Vec::new();
    let mut coverage = SearchCoverageDto::default();
    for (source, _) in ready {
        if !selected.is_empty() && !selected.contains(source.id.0.as_str()) {
            continue;
        }
        if let Some(source) = context.load(source, &mut coverage)? {
            sources.push(source);
        }
    }
    coverage.missing_source_ids.sort_unstable();
    coverage.missing_source_ids.dedup();
    coverage.complete = coverage.ready_source_count == coverage.indexed_source_count
        && coverage.expected_entry_count == coverage.indexed_entry_count
        && coverage.missing_source_ids.is_empty();
    sources.sort_unstable_by(|left, right| left.data_source_id.0.cmp(&right.data_source_id.0));
    Ok(SearchSourceSet { sources, coverage })
}

struct SourceQuery<'a> {
    case_conn: &'a Connection,
    case_root: &'a Path,
    canonical_root: &'a Path,
    case_id: &'a CaseId,
    options: &'a FileSearchOptions,
}

impl SourceQuery<'_> {
    fn load(
        &self,
        source: DataSource,
        coverage: &mut SearchCoverageDto,
    ) -> Result<Option<SearchSource>, SearchError> {
        coverage.ready_source_count = coverage.ready_source_count.saturating_add(1);
        let expected = crate::source_db::ready_source_catalog_count(self.case_root, &source.id)
            .map_err(|error| SearchError::Other(error.to_string()))?;
        coverage.expected_entry_count = coverage.expected_entry_count.saturating_add(expected);
        let index_dir = registered_source_index_dir(self.case_conn, self.case_root, &source.id)?;
        if !index_dir.exists() {
            coverage.missing_source_ids.push(source.id.0.clone());
            return Ok(None);
        }
        let safe_index_dir = safe_existing_case_path(self.case_root, &index_dir)?;
        let session = match pool::query_session(
            self.canonical_root,
            &self.case_id.0,
            &source.id.0,
            &safe_index_dir,
            self.options,
        ) {
            Ok(session) => session,
            Err(SearchError::Index(error)) => {
                tracing::warn!(data_source_id = %source.id.0, error = %error, "File search index is unavailable; re-run data-source analysis to rebuild the index");
                coverage.missing_source_ids.push(source.id.0.clone());
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let indexed = session.document_count();
        coverage.indexed_entry_count = coverage.indexed_entry_count.saturating_add(indexed);
        coverage.indexed_source_count = coverage.indexed_source_count.saturating_add(1);
        if indexed != expected {
            coverage.missing_source_ids.push(source.id.0.clone());
        }
        Ok(Some(SearchSource {
            data_source_id: source.id,
            data_source_name: source.name,
            session,
        }))
    }
}
