use super::SearchCache;
use crate::search_service::SearchError;

impl SearchCache {
    fn counts(&self) -> Result<(usize, usize, u64), SearchError> {
        let state = self.state.lock().map_err(super::lock_error)?;
        Ok((
            state.indexes.len(),
            state.queries.len(),
            self.resources.snapshot().0.readers,
        ))
    }
}

#[test]
fn search_cache_permits_remain_reserved_until_borrowers_release_them() {
    let cache = SearchCache::new();
    let permits = (0..256)
        .map(|_| cache.reserve(0).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(cache.counts().unwrap(), (0, 0, 256));
    assert!(matches!(cache.reserve(0), Err(SearchError::Resources(_))));
    drop(permits);
    assert_eq!(cache.counts().unwrap(), (0, 0, 0));
    let _permit = cache.reserve(0).unwrap();
    assert_eq!(cache.counts().unwrap(), (0, 0, 1));
}

#[test]
fn search_cache_estimated_bytes_reject_pressure_before_reader_admission() {
    let cache = SearchCache::new();
    let permit = cache.reserve(512 * 1024 * 1024).unwrap();
    assert!(matches!(cache.reserve(1), Err(SearchError::Resources(_))));
    drop(permit);
    assert!(cache.reserve(1).is_ok());
    assert!(matches!(
        cache.reserve(512 * 1024 * 1024 + 1),
        Err(SearchError::Resources(_))
    ));
}
