use super::budget::WORKSPACE_BYTES;
use super::entry::{CachedIndex, CachedSession, IndexKey, QueryKey};
use crate::resource_budget::{ResourceError, ResourcePermit, ResourcePool, Resources};
use crate::search_service::SearchError;
use lru::LruCache;
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

pub(super) type IndexSlot = Arc<Mutex<Option<Arc<CachedIndex>>>>;
pub(super) type QuerySlot = Arc<Mutex<Option<Arc<CachedSession>>>>;

const CACHE_CAPACITY: NonZeroUsize = NonZeroUsize::new(64).unwrap();
const LIVE_OBJECT_LIMIT: u64 = 256;

struct State {
    indexes: LruCache<IndexKey, IndexSlot>,
    queries: LruCache<QueryKey, QuerySlot>,
}

pub(super) struct SearchCache {
    state: Mutex<State>,
    resources: ResourcePool,
}

enum Evicted {
    Index(IndexSlot),
    Query(QuerySlot),
}

impl SearchCache {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(State {
                indexes: LruCache::new(CACHE_CAPACITY),
                queries: LruCache::new(CACHE_CAPACITY),
            }),
            resources: ResourcePool::new(Resources {
                bytes: WORKSPACE_BYTES,
                readers: LIVE_OBJECT_LIMIT,
                ..Default::default()
            }),
        }
    }

    pub(super) fn index_slot(&self, key: &IndexKey) -> Result<IndexSlot, SearchError> {
        let (slot, evicted) = {
            let mut state = self.state.lock().map_err(lock_error)?;
            if let Some(slot) = state.indexes.get(key) {
                return Ok(slot.clone());
            }
            let slot = Arc::new(Mutex::new(None));
            let evicted = state.indexes.push(key.clone(), slot.clone());
            (slot, evicted)
        };
        drop(evicted);
        Ok(slot)
    }

    pub(super) fn query_slot(&self, key: &QueryKey) -> Result<QuerySlot, SearchError> {
        let (slot, evicted) = {
            let mut state = self.state.lock().map_err(lock_error)?;
            if let Some(slot) = state.queries.get(key) {
                return Ok(slot.clone());
            }
            let slot = Arc::new(Mutex::new(None));
            let evicted = state.queries.push(key.clone(), slot.clone());
            (slot, evicted)
        };
        drop(evicted);
        Ok(slot)
    }

    pub(super) fn reserve(&self, bytes: u64) -> Result<ResourcePermit, SearchError> {
        loop {
            match self.resources.acquire(
                Resources {
                    bytes,
                    readers: 1,
                    ..Default::default()
                },
                None,
            ) {
                Ok(permit) => return Ok(permit),
                Err(ResourceError::Busy) => {
                    let evicted = {
                        let mut state = self.state.lock().map_err(lock_error)?;
                        state
                            .queries
                            .pop_lru()
                            .map(|(_, slot)| Evicted::Query(slot))
                            .or_else(|| {
                                state
                                    .indexes
                                    .pop_lru()
                                    .map(|(_, slot)| Evicted::Index(slot))
                            })
                    };
                    match evicted {
                        Some(Evicted::Index(slot)) => drop(slot),
                        Some(Evicted::Query(slot)) => drop(slot),
                        None => return Err(ResourceError::Busy.into()),
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub(super) fn remove_where(
        &self,
        predicate: impl Fn(&IndexKey) -> bool,
    ) -> Result<(), SearchError> {
        let (indexes, queries) = {
            let mut state = self.state.lock().map_err(lock_error)?;
            let index_keys = state
                .indexes
                .iter()
                .filter(|(key, _)| predicate(key))
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            let query_keys = state
                .queries
                .iter()
                .filter(|(key, _)| predicate(&key.index))
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            let indexes = index_keys
                .iter()
                .filter_map(|key| state.indexes.pop(key))
                .collect::<Vec<_>>();
            let queries = query_keys
                .iter()
                .filter_map(|key| state.queries.pop(key))
                .collect::<Vec<_>>();
            (indexes, queries)
        };
        drop(queries);
        drop(indexes);
        Ok(())
    }

    pub(super) fn remove_stale_queries(&self, key: &QueryKey) -> Result<(), SearchError> {
        let queries = {
            let mut state = self.state.lock().map_err(lock_error)?;
            let keys = state
                .queries
                .iter()
                .filter(|(existing, _)| {
                    existing.index == key.index && existing.opstamp != key.opstamp
                })
                .map(|(existing, _)| existing.clone())
                .collect::<Vec<_>>();
            keys.iter()
                .filter_map(|key| state.queries.pop(key))
                .collect::<Vec<_>>()
        };
        drop(queries);
        Ok(())
    }

    pub(super) fn remove_query(&self, key: &QueryKey) -> Result<(), SearchError> {
        let removed = self.state.lock().map_err(lock_error)?.queries.pop(key);
        drop(removed);
        Ok(())
    }

    pub(super) fn remove_index(&self, key: &IndexKey) -> Result<(), SearchError> {
        let removed = self.state.lock().map_err(lock_error)?.indexes.pop(key);
        drop(removed);
        Ok(())
    }
}

pub(super) fn lock_error<T>(_error: std::sync::PoisonError<T>) -> SearchError {
    SearchError::Other("file search cache lock is poisoned".to_owned())
}

#[cfg(test)]
#[path = "../../../../tests/unit/search_service/cache_budget.rs"]
mod tests;
