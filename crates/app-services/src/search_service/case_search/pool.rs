mod budget;
mod cache;
mod entry;

use cache::{lock_error, SearchCache};
pub(super) use entry::CachedSession;
use entry::{CachedIndex, IndexKey, QueryKey};
use persistence_source::fjall::instrumentation;
use search::{FileSearchOptions, SearchIndex};
use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, LazyLock},
};

use crate::search_service::SearchError;

static CACHE: LazyLock<SearchCache> = LazyLock::new(SearchCache::new);

pub(super) fn query_session(
    case_root: &Path,
    case_id: &str,
    source_id: &str,
    index_dir: &Path,
    options: &FileSearchOptions,
) -> Result<Arc<CachedSession>, SearchError> {
    let (generation, schema) = SearchIndex::identity_from_path(index_dir).map_err(index_error)?;
    let key = IndexKey {
        case_root: case_root.to_path_buf(),
        case_id: case_id.to_owned(),
        source_id: source_id.to_owned(),
        index_dir: index_dir.to_path_buf(),
        generation,
        schema,
    };
    CACHE.remove_where(|existing| {
        existing.index_dir == key.index_dir
            && (existing.generation != key.generation || existing.schema != key.schema)
    })?;
    let index = cached_index(&key)?;
    let workspace = budget::query_workspace(&key, options)?;
    let key = QueryKey {
        opstamp: index.index.snapshot_opstamp().map_err(index_error)?,
        index: key,
        options: options.clone(),
    };
    CACHE.remove_stale_queries(&key)?;
    let slot = CACHE.query_slot(&key)?;
    let mut stored = slot.lock().map_err(lock_error)?;
    if let Some(session) = &*stored {
        return Ok(session.clone());
    }
    let session = match build_session(index, &key, workspace) {
        Ok(session) => session,
        Err(error) => {
            CACHE.remove_query(&key)?;
            return Err(error);
        }
    };
    *stored = Some(session.clone());
    Ok(session)
}

fn build_session(
    index: Arc<CachedIndex>,
    key: &QueryKey,
    workspace: u64,
) -> Result<Arc<CachedSession>, SearchError> {
    let permit = CACHE.reserve(workspace)?;
    let session = index
        .index
        .file_query_session(&key.options)
        .map_err(index_error)?;
    instrumentation::record_query_session_open();
    if session.snapshot_opstamp() != key.opstamp {
        return Err(SearchError::Index(
            "file index changed while creating a search session".to_owned(),
        ));
    }
    let session = Arc::new(CachedSession {
        session,
        _index: index,
        _permit: permit,
    });
    Ok(session)
}

fn cached_index(key: &IndexKey) -> Result<Arc<CachedIndex>, SearchError> {
    let slot = CACHE.index_slot(key)?;
    let mut stored = slot.lock().map_err(lock_error)?;
    if let Some(index) = &*stored {
        return Ok(index.clone());
    }
    let index = match build_index(key) {
        Ok(index) => index,
        Err(error) => {
            CACHE.remove_index(key)?;
            return Err(error);
        }
    };
    *stored = Some(index.clone());
    Ok(index)
}

fn build_index(key: &IndexKey) -> Result<Arc<CachedIndex>, SearchError> {
    let permit = CACHE.reserve(budget::index_workspace(key)?)?;
    let index = SearchIndex::open(&key.index_dir).map_err(index_error)?;
    instrumentation::record_search_index_open();
    index.validate_file_search_schema().map_err(index_error)?;
    if index.generation() != key.generation || index.schema_version() != key.schema {
        return Err(SearchError::Index(
            "file index identity changed while opening it".to_owned(),
        ));
    }
    let index = Arc::new(CachedIndex {
        index,
        _permit: permit,
    });
    Ok(index)
}

pub fn clear_case_file_search_cache(case_id: &str) -> Result<(), SearchError> {
    CACHE.remove_where(|key| key.case_id == case_id)
}

pub fn clear_source_file_search_cache(
    case_root: &Path,
    source_id: &str,
) -> Result<(), SearchError> {
    let root = case_root
        .canonicalize()
        .map_err(|error| SearchError::Other(error.to_string()))?;
    CACHE.remove_where(|key| key.case_root == root && key.source_id == source_id)
}

pub(super) fn retain_ready_sources(
    case_root: &Path,
    case_id: &str,
    source_ids: &HashSet<&str>,
) -> Result<(), SearchError> {
    CACHE.remove_where(|key| {
        key.case_root == case_root
            && key.case_id == case_id
            && !source_ids.contains(key.source_id.as_str())
    })
}

fn index_error(error: search::indexer::tantivy_writer::IndexError) -> SearchError {
    SearchError::Index(error.to_string())
}
