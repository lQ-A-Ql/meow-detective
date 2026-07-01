//! E01 reader cache: per-case LRU cache of parsed E01 readers.

use image_e01::E01Reader;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

/// Maximum number of concurrently cached parsed E01 readers per case.
/// Cache hits reuse the `Arc<chunk_table>` via `E01Reader::re_open`,
/// opening fresh segment file handles without re-parsing headers.
/// Bucketing by case prevents one case's preview activity from evicting
/// another case's readers.
pub(crate) const E01_READER_CACHE_PER_CASE_MAX_SIZE: usize = 16;

struct E01ReaderCache {
    max_size: usize,
    paths: VecDeque<PathBuf>,
    readers: HashMap<PathBuf, E01Reader>,
}

impl E01ReaderCache {
    fn new(max_size: usize) -> Self {
        Self {
            max_size,
            paths: VecDeque::with_capacity(max_size),
            readers: HashMap::with_capacity(max_size),
        }
    }

    fn get_or_open(&mut self, source_path: &Path) -> std::io::Result<E01Reader> {
        // Cache hit: re-open fresh file handles, share Arc<chunk_table>
        if let Some(cached) = self.readers.get(source_path) {
            // Update LRU: move to most-recently-used end
            if let Some(pos) = self.paths.iter().position(|p| p == source_path) {
                self.paths.remove(pos);
            }
            self.paths.push_back(source_path.to_path_buf());
            return cached.re_open(source_path);
        }

        // Cache miss: fully parse from disk
        let reader = E01Reader::open(source_path)?;

        // Evict oldest if full
        while self.paths.len() >= self.max_size {
            if let Some(evict_path) = self.paths.pop_front() {
                self.readers.remove(&evict_path);
            }
        }

        self.paths.push_back(source_path.to_path_buf());
        self.readers.insert(source_path.to_path_buf(), reader);

        // Re-open for the caller with fresh handles
        self.readers
            .get(source_path)
            .expect("reader was just inserted")
            .re_open(source_path)
    }
}

/// Per-case E01 reader cache. Callers that do not have a case context (for
/// example low-level filesystem probes) use the empty-string default bucket.
pub(crate) struct E01ReaderCacheRegistry {
    default_bucket: E01ReaderCache,
    buckets: HashMap<String, E01ReaderCache>,
}

impl E01ReaderCacheRegistry {
    fn new(max_size: usize) -> Self {
        Self {
            default_bucket: E01ReaderCache::new(max_size),
            buckets: HashMap::new(),
        }
    }

    fn bucket(&mut self, case_id: &str) -> &mut E01ReaderCache {
        if case_id.is_empty() {
            return &mut self.default_bucket;
        }
        self.buckets
            .entry(case_id.to_string())
            .or_insert_with(|| E01ReaderCache::new(E01_READER_CACHE_PER_CASE_MAX_SIZE))
    }

    fn clear(&mut self) {
        self.default_bucket.paths.clear();
        self.default_bucket.readers.clear();
        self.buckets.clear();
    }

    fn clear_case(&mut self, case_id: &str) {
        if case_id.is_empty() {
            self.default_bucket.paths.clear();
            self.default_bucket.readers.clear();
            return;
        }
        if let Some(bucket) = self.buckets.remove(case_id) {
            drop(bucket);
        }
    }
}

pub(crate) static E01_READER_CACHE: std::sync::LazyLock<std::sync::Mutex<E01ReaderCacheRegistry>> =
    std::sync::LazyLock::new(|| {
        std::sync::Mutex::new(E01ReaderCacheRegistry::new(
            E01_READER_CACHE_PER_CASE_MAX_SIZE,
        ))
    });

/// Clear the global E01 reader cache.
pub fn clear_e01_reader_cache() {
    if let Ok(mut cache) = E01_READER_CACHE.lock() {
        cache.clear();
    }
}

/// Clear cached E01 readers for a single case.
pub fn clear_e01_reader_cache_for_case(case_id: &str) {
    if let Ok(mut cache) = E01_READER_CACHE.lock() {
        cache.clear_case(case_id);
    }
}

pub(crate) fn open_e01_reader_cached(
    source_path: &Path,
    case_id: &str,
) -> std::io::Result<E01Reader> {
    let mut registry = E01_READER_CACHE.lock().unwrap_or_else(|poisoned| {
        // Clear the cache on poison to avoid using potentially corrupted state
        let mut registry = poisoned.into_inner();
        registry.clear();
        registry
    });
    registry.bucket(case_id).get_or_open(source_path)
}
