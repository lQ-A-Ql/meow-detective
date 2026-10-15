//! Per-case cache for parsed E01 metadata and chunk tables.

use image_e01::{E01OpenPlan, E01Reader};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

pub(crate) const E01_READER_CACHE_PER_CASE_MAX_SIZE: usize = 16;

struct E01ReaderCache {
    max_size: usize,
    paths: VecDeque<PathBuf>,
    plans: HashMap<PathBuf, E01OpenPlan>,
}

impl E01ReaderCache {
    fn new(max_size: usize) -> Self {
        Self {
            max_size,
            paths: VecDeque::with_capacity(max_size),
            plans: HashMap::with_capacity(max_size),
        }
    }

    fn get_or_open(&mut self, source_path: &Path) -> std::io::Result<E01Reader> {
        if let Some(cached) = self.plans.get(source_path) {
            if let Some(position) = self.paths.iter().position(|path| path == source_path) {
                self.paths.remove(position);
            }
            self.paths.push_back(source_path.to_path_buf());
            return cached.open_reader();
        }

        let plan = E01Reader::open(source_path)?.open_plan()?;
        while self.paths.len() >= self.max_size {
            if let Some(evicted_path) = self.paths.pop_front() {
                self.plans.remove(&evicted_path);
            }
        }
        self.paths.push_back(source_path.to_path_buf());
        self.plans.insert(source_path.to_path_buf(), plan);
        self.plans
            .get(source_path)
            .expect("plan was just inserted")
            .open_reader()
    }
}

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
        self.default_bucket.plans.clear();
        self.buckets.clear();
    }

    fn clear_case(&mut self, case_id: &str) {
        if case_id.is_empty() {
            self.default_bucket.paths.clear();
            self.default_bucket.plans.clear();
        } else {
            self.buckets.remove(case_id);
        }
    }
}

pub(crate) static E01_READER_CACHE: std::sync::LazyLock<std::sync::Mutex<E01ReaderCacheRegistry>> =
    std::sync::LazyLock::new(|| {
        std::sync::Mutex::new(E01ReaderCacheRegistry::new(
            E01_READER_CACHE_PER_CASE_MAX_SIZE,
        ))
    });

pub fn clear_e01_reader_cache() {
    if let Ok(mut cache) = E01_READER_CACHE.lock() {
        cache.clear();
    }
}

pub fn clear_e01_reader_cache_for_case(case_id: &str) {
    if let Ok(mut cache) = E01_READER_CACHE.lock() {
        cache.clear_case(case_id);
    }
}

pub fn open_e01_reader_cached(source_path: &Path, case_id: &str) -> std::io::Result<E01Reader> {
    let mut registry = E01_READER_CACHE.lock().unwrap_or_else(|poisoned| {
        let mut registry = poisoned.into_inner();
        registry.clear();
        registry
    });
    registry.bucket(case_id).get_or_open(source_path)
}
