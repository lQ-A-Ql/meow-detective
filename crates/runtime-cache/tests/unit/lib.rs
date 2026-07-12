use super::*;
use chrono::Duration;
use std::sync::{Arc, Mutex};

#[test]
fn runtime_cache_full_workflow() {
    let cache = RuntimeCache::open_in_memory().unwrap();

    // Test cache entry
    let entry = cache
        .cache()
        .get_or_insert("test-key", "test", Duration::seconds(60), || {
            Ok(serde_json::json!({"value": 42}))
        })
        .unwrap();
    assert_eq!(entry.value_json, serde_json::json!({"value": 42}));

    // Test file handle
    let handle_id = cache
        .handles()
        .create("case-1", "file-1", Duration::minutes(30))
        .unwrap();
    let handle = cache.handles().get(&handle_id).unwrap().unwrap();
    assert_eq!(handle.case_id, "case-1");

    // Test cleanup
    let cleaned = cache.cleanup_all().unwrap();
    assert_eq!(cleaned, 0); // Nothing expired yet

    // Test clear case
    let cleared = cache.clear_case("case-1").unwrap();
    assert_eq!(cleared, 1); // One handle cleared
}

#[test]
fn test_cache_insert_and_get() {
    let cache = RuntimeCache::open_in_memory().unwrap();
    let repo = cache.cache();

    // Insert using get_or_insert
    let entry = repo
        .get_or_insert("insert-key", "insert-ns", Duration::seconds(300), || {
            Ok(serde_json::json!({"data": "hello"}))
        })
        .unwrap();
    assert_eq!(entry.value_json, serde_json::json!({"data": "hello"}));
    assert_eq!(entry.cache_key, "insert-key");
    assert_eq!(entry.namespace, "insert-ns");

    // Retrieve using get
    let loaded = repo.get("insert-key").unwrap().unwrap();
    assert_eq!(loaded.value_json, serde_json::json!({"data": "hello"}));
    assert_eq!(loaded.cache_key, "insert-key");
}

#[test]
fn test_cache_miss_returns_none() {
    let cache = RuntimeCache::open_in_memory().unwrap();

    let result = cache.cache().get("no-such-key").unwrap();
    assert!(result.is_none(), "Missing key should return None");
}

#[test]
fn test_cache_eviction_lru() {
    // Note: the current cache uses TTL-based expiration rather than
    // strict LRU. This test verifies that expired entries are evicted
    // and not returned to callers.
    let cache = RuntimeCache::open_in_memory().unwrap();

    // Insert an entry that is already expired
    let past = chrono::Utc::now() - Duration::minutes(10);
    let expired_entry = CacheEntry {
        cache_key: "stale-entry".to_string(),
        namespace: "evict-ns".to_string(),
        case_id: None,
        value_json: serde_json::json!({"old": true}),
        created_at: past,
        expires_at: Some(past + chrono::Duration::seconds(30)),
        last_accessed_at: past,
    };
    cache.cache().set(&expired_entry).unwrap();

    // Expired entry should not be returned
    let result = cache.cache().get("stale-entry").unwrap();
    assert!(result.is_none(), "Expired entry should not be returned");

    // Cleanup should remove the expired row
    let cleaned = cache.cache().cleanup_expired().unwrap();
    assert_eq!(cleaned, 1, "One expired entry should be cleaned up");
}

#[test]
fn test_cache_clear() {
    let cache = RuntimeCache::open_in_memory().unwrap();

    // Insert entries with different namespaces
    cache
        .cache()
        .get_or_insert("k1", "ns-a", Duration::seconds(300), || {
            Ok(serde_json::json!(1))
        })
        .unwrap();
    cache
        .cache()
        .get_or_insert("k2", "ns-a", Duration::seconds(300), || {
            Ok(serde_json::json!(2))
        })
        .unwrap();
    cache
        .cache()
        .get_or_insert("k3", "ns-b", Duration::seconds(300), || {
            Ok(serde_json::json!(3))
        })
        .unwrap();

    // Clear namespace "ns-a"
    let cleared = cache.cache().clear_namespace("ns-a").unwrap();
    assert_eq!(cleared, 2, "Two entries in ns-a should be cleared");

    // ns-a entries should be gone
    assert!(cache.cache().get("k1").unwrap().is_none());
    assert!(cache.cache().get("k2").unwrap().is_none());

    // ns-b entry should still exist
    let remaining = cache.cache().get("k3").unwrap();
    assert!(remaining.is_some(), "ns-b entry should survive ns-a clear");
    assert_eq!(remaining.unwrap().value_json, serde_json::json!(3));
}

#[test]
fn test_cache_capacity_limit() {
    // There is no hard capacity limit — the cache uses TTL-based expiry.
    // This test verifies that a reasonable volume of entries can be
    // inserted and retrieved without data loss or unexpected eviction.
    let cache = RuntimeCache::open_in_memory().unwrap();

    let count = 200;
    for i in 0..count {
        cache
            .cache()
            .get_or_insert(
                &format!("cap-key-{}", i),
                "capacity-ns",
                Duration::minutes(30),
                || Ok(serde_json::json!({"index": i})),
            )
            .unwrap();
    }

    // Verify all entries exist
    for i in 0..count {
        let key = format!("cap-key-{}", i);
        let entry = cache.cache().get(&key).unwrap();
        assert!(entry.is_some(), "Entry {} should exist", key);
        assert_eq!(entry.unwrap().value_json, serde_json::json!({"index": i}));
    }
}

#[test]
fn test_concurrent_access() {
    use std::thread;

    let cache = Arc::new(Mutex::new(RuntimeCache::open_in_memory().unwrap()));
    let thread_count = 4;
    let mut handles = vec![];

    // Each thread inserts its own key then reads it back
    for thread_id in 0..thread_count {
        let cache = Arc::clone(&cache);
        let handle = thread::spawn(move || {
            let key = format!("concurrent-key-{}", thread_id);
            {
                let cache = cache.lock().unwrap_or_else(|e| e.into_inner());
                cache
                    .cache()
                    .get_or_insert(&key, "concurrent-ns", Duration::minutes(5), || {
                        Ok(serde_json::json!({"thread": thread_id}))
                    })
                    .unwrap();
            }
            // Read it back
            {
                let cache = cache.lock().unwrap_or_else(|e| e.into_inner());
                let entry = cache.cache().get(&key).unwrap();
                assert!(
                    entry.is_some(),
                    "Entry for thread {} should exist",
                    thread_id
                );
                assert_eq!(
                    entry.unwrap().value_json,
                    serde_json::json!({"thread": thread_id})
                );
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}
