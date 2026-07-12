use super::*;
use std::cell::Cell;

#[test]
fn cache_set_get_delete() {
    let conn = crate::connection::open_in_memory().unwrap();
    let repo = CacheRepo::new(&conn);

    let entry = CacheEntry {
        cache_key: "test-key".to_string(),
        namespace: "test".to_string(),
        case_id: None,
        value_json: serde_json::json!({"data": "value"}),
        created_at: Utc::now(),
        expires_at: None,
        last_accessed_at: Utc::now(),
    };

    repo.set(&entry).unwrap();

    let loaded = repo.get("test-key").unwrap().unwrap();
    assert_eq!(loaded.cache_key, "test-key");
    assert_eq!(loaded.value_json, serde_json::json!({"data": "value"}));

    repo.delete("test-key").unwrap();
    assert!(repo.get("test-key").unwrap().is_none());
}

#[test]
fn cache_get_or_insert() {
    let conn = crate::connection::open_in_memory().unwrap();
    let repo = CacheRepo::new(&conn);

    let entry = repo
        .get_or_insert("key1", "ns", Duration::seconds(60), || {
            Ok(serde_json::json!({"computed": 42}))
        })
        .unwrap();

    assert_eq!(entry.value_json, serde_json::json!({"computed": 42}));

    // Second call should return cached
    let entry2 = repo
        .get_or_insert("key1", "ns", Duration::seconds(60), || {
            unreachable!("cache hit should not invoke factory")
        })
        .unwrap();

    assert_eq!(entry2.value_json, serde_json::json!({"computed": 42}));
}

#[test]
fn cache_get_or_insert_case_sets_case_id_and_clear_case_removes_it() {
    let conn = crate::connection::open_in_memory().unwrap();
    let repo = CacheRepo::new(&conn);

    let entry = repo
        .get_or_insert_case(
            "case-key",
            crate::models::namespaces::PREVIEW_DESCRIPTORS,
            "case-1",
            Duration::seconds(60),
            || Ok(serde_json::json!({"descriptor": true})),
        )
        .unwrap();

    assert_eq!(entry.case_id.as_deref(), Some("case-1"));
    assert_eq!(repo.clear_case("case-1").unwrap(), 1);
    assert!(repo.get("case-key").unwrap().is_none());
}

#[test]
fn cache_get_or_insert_case_hits_until_clear_case_invalidates() {
    let conn = crate::connection::open_in_memory().unwrap();
    let repo = CacheRepo::new(&conn);
    let factory_calls = Cell::new(0usize);

    let first = repo
        .get_or_insert_case(
            "descriptor-key",
            crate::models::namespaces::PREVIEW_DESCRIPTORS,
            "case-1",
            Duration::seconds(60),
            || {
                factory_calls.set(factory_calls.get() + 1);
                Ok(serde_json::json!({"generation": factory_calls.get()}))
            },
        )
        .unwrap();
    assert_eq!(first.value_json, serde_json::json!({"generation": 1}));
    assert_eq!(factory_calls.get(), 1);

    let second = repo
        .get_or_insert_case(
            "descriptor-key",
            crate::models::namespaces::PREVIEW_DESCRIPTORS,
            "case-1",
            Duration::seconds(60),
            || {
                factory_calls.set(factory_calls.get() + 1);
                Ok(serde_json::json!({"generation": factory_calls.get()}))
            },
        )
        .unwrap();
    assert_eq!(second.value_json, serde_json::json!({"generation": 1}));
    assert_eq!(factory_calls.get(), 1);

    assert_eq!(repo.clear_case("case-1").unwrap(), 1);

    let third = repo
        .get_or_insert_case(
            "descriptor-key",
            crate::models::namespaces::PREVIEW_DESCRIPTORS,
            "case-1",
            Duration::seconds(60),
            || {
                factory_calls.set(factory_calls.get() + 1);
                Ok(serde_json::json!({"generation": factory_calls.get()}))
            },
        )
        .unwrap();
    assert_eq!(third.value_json, serde_json::json!({"generation": 2}));
    assert_eq!(factory_calls.get(), 2);
}

#[test]
fn cache_get_or_insert_case_does_not_hit_different_namespace() {
    let conn = crate::connection::open_in_memory().unwrap();
    let repo = CacheRepo::new(&conn);
    let factory_calls = Cell::new(0usize);

    let first = repo
        .get_or_insert_case(
            "shared-key",
            "search_results",
            "case-1",
            Duration::seconds(60),
            || {
                factory_calls.set(factory_calls.get() + 1);
                Ok(serde_json::json!({"namespace": "search"}))
            },
        )
        .unwrap();
    assert_eq!(first.value_json, serde_json::json!({"namespace": "search"}));

    let second = repo
        .get_or_insert_case(
            "shared-key",
            crate::models::namespaces::PREVIEW_DESCRIPTORS,
            "case-1",
            Duration::seconds(60),
            || {
                factory_calls.set(factory_calls.get() + 1);
                Ok(serde_json::json!({"namespace": "preview"}))
            },
        )
        .unwrap();

    assert_eq!(
        second.value_json,
        serde_json::json!({"namespace": "preview"})
    );
    assert_eq!(
        second.namespace,
        crate::models::namespaces::PREVIEW_DESCRIPTORS
    );
    assert_eq!(factory_calls.get(), 2);
}

#[test]
fn cache_cleanup_expired() {
    let conn = crate::connection::open_in_memory().unwrap();
    let repo = CacheRepo::new(&conn);

    let past = Utc::now() - Duration::seconds(60);
    let entry = CacheEntry {
        cache_key: "expired".to_string(),
        namespace: "test".to_string(),
        case_id: None,
        value_json: serde_json::json!(null),
        created_at: past,
        expires_at: Some(past + Duration::seconds(30)),
        last_accessed_at: past,
    };
    repo.set(&entry).unwrap();

    assert!(repo.get("expired").unwrap().is_none());

    let cleaned = repo.cleanup_expired().unwrap();
    assert_eq!(cleaned, 1);
}
