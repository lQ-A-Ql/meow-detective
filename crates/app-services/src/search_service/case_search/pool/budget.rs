use super::entry::IndexKey;
use crate::{resource_budget::ResourceError, search_service::SearchError};
use search::FileSearchOptions;

pub(super) const WORKSPACE_BYTES: u64 = 512 * 1024 * 1024;
const INDEX_BASE_BYTES: u64 = 1024 * 1024;
const SESSION_BASE_BYTES: u64 = 512 * 1024;
const QUERY_EXPANSION_FACTOR: u64 = 64;

// These are retained-workspace estimates, not limits on process RSS or mapped
// index bytes. The query allowance covers owned terms, compiled query state,
// match-source weights and snippets. Actual RSS is measured by the benchmark.
pub(super) fn index_workspace(key: &IndexKey) -> Result<u64, SearchError> {
    INDEX_BASE_BYTES
        .checked_add(key_bytes(key)?)
        .ok_or_else(limit)
}

pub(super) fn query_workspace(
    key: &IndexKey,
    options: &FileSearchOptions,
) -> Result<u64, SearchError> {
    let input = options
        .extensions
        .iter()
        .try_fold(options.query.len() as u64, |size, item| {
            size.checked_add(item.len() as u64).ok_or_else(limit)
        })?;
    let expanded = input
        .checked_mul(QUERY_EXPANSION_FACTOR)
        .ok_or_else(limit)?;
    let key_bytes = key_bytes(key)?;
    let estimate = SESSION_BASE_BYTES
        .checked_add(expanded)
        .and_then(|bytes| bytes.checked_add(key_bytes))
        .ok_or_else(limit)?;
    if estimate > WORKSPACE_BYTES {
        return Err(limit());
    }
    Ok(estimate)
}

fn key_bytes(key: &IndexKey) -> Result<u64, SearchError> {
    let sizes = [
        key.case_root.as_os_str().len(),
        key.case_id.len(),
        key.source_id.len(),
        key.index_dir.as_os_str().len(),
        key.generation.len(),
    ];
    sizes
        .into_iter()
        .try_fold(0u64, |total, size| {
            total.checked_add(size as u64).ok_or_else(limit)
        })?
        .checked_mul(2)
        .ok_or_else(limit)
}

fn limit() -> SearchError {
    ResourceError::Limit.into()
}
