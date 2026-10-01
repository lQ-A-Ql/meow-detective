//! cache support for source-bound reads.
use super::*;
pub(super) fn cache_preview_descriptor(
    cache: &mut HashMap<String, Value>,
    key: &str,
    value: &Value,
) {
    if cache.len() >= MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES && !cache.contains_key(key) {
        cache.clear();
    }
    cache.insert(key.to_string(), value.clone());
}

pub(super) fn cache_partition_candidates(
    cache: &mut HashMap<usize, Vec<crate::file_service::viewer::PreviewPartitionCandidate>>,
    partition_index: usize,
    candidates: Vec<crate::file_service::viewer::PreviewPartitionCandidate>,
) {
    if cache.len() >= MAX_SOURCE_PARTITION_CACHE_ENTRIES && !cache.contains_key(&partition_index) {
        cache.clear();
    }
    cache.insert(partition_index, candidates);
}
