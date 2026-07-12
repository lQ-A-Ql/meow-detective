use super::BlockCache;

#[test]
fn cache_respects_byte_derived_capacity() {
    let mut cache = BlockCache::with_byte_budget(4, 8);
    cache.insert(1, vec![1; 4]);
    cache.insert(2, vec![2; 4]);
    assert!(cache.get(1).is_some());

    cache.insert(3, vec![3; 4]);
    assert!(cache.get(1).is_none());
    assert!(cache.get(2).is_some());
    assert!(cache.get(3).is_some());
}
