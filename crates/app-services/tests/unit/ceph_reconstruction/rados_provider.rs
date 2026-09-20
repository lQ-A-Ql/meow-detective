use std::path::PathBuf;

use domain::DataSourceId;

use super::cache::{copy_verified_segment, VerifiedObject, VerifiedObjectCache, PAGE_BYTES};
use super::*;

#[test]
fn rejects_empty_replica_coverage() {
    let error = SourceDbRadosObjectProvider::new(Vec::new(), 8, Vec::new(), 3)
        .err()
        .expect("empty coverage should fail");

    assert!(matches!(error, RadosProviderError::CoverageNotClosed));
}

#[test]
fn rejects_duplicate_inventory_bindings() {
    let first = RadosReplicaSource::new(
        DataSourceId("source-a".to_string()),
        "inventory-a",
        PathBuf::from("sources/a/source.db"),
    )
    .unwrap();
    let second = RadosReplicaSource::new(
        DataSourceId("source-b".to_string()),
        "inventory-a",
        PathBuf::from("sources/b/source.db"),
    )
    .unwrap();
    let third = replica("source-c", "inventory-c");

    let error = SourceDbRadosObjectProvider::new(vec![first, second, third], 8, Vec::new(), 3)
        .err()
        .expect("duplicate inventory should fail");

    assert!(matches!(
        error,
        RadosProviderError::DuplicateInventory { inventory_id } if inventory_id == "inventory-a"
    ));
}

#[test]
fn rejects_duplicate_data_source_bindings() {
    let first = RadosReplicaSource::new(
        DataSourceId("source-a".to_string()),
        "inventory-a",
        PathBuf::from("sources/a/source.db"),
    )
    .unwrap();
    let second = RadosReplicaSource::new(
        DataSourceId("source-a".to_string()),
        "inventory-b",
        PathBuf::from("sources/b/source.db"),
    )
    .unwrap();
    let third = replica("source-c", "inventory-c");

    let error = SourceDbRadosObjectProvider::new(vec![first, second, third], 8, Vec::new(), 3)
        .err()
        .expect("duplicate data source should fail");

    assert!(matches!(
        error,
        RadosProviderError::DuplicateSource { data_source_id } if data_source_id == "source-a"
    ));
}

#[test]
fn rejects_non_closed_replica_count() {
    let source = RadosReplicaSource::new(
        DataSourceId("source-a".to_string()),
        "inventory-a",
        PathBuf::from("sources/a/source.db"),
    )
    .unwrap();

    let error = SourceDbRadosObjectProvider::new(vec![source], 8, Vec::new(), 3)
        .err()
        .expect("incomplete coverage should fail");

    assert!(matches!(error, RadosProviderError::CoverageNotClosed));
}

#[test]
fn rejects_indeterminate_replica_identity_even_when_count_is_closed() {
    let incomplete = RadosReplicaSource::new(
        DataSourceId("source-a".to_string()),
        "inventory-a",
        PathBuf::from("sources/a/source.db"),
    )
    .unwrap();
    let result = SourceDbRadosObjectProvider::new(
        vec![
            incomplete,
            replica("source-b", "inventory-b"),
            replica("source-c", "inventory-c"),
        ],
        8,
        Vec::new(),
        3,
    );
    let error = match result {
        Ok(_) => panic!("incomplete identity must fail closed"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        RadosProviderError::CoverageNotProven { .. }
    ));
}

#[test]
fn source_db_open_failure_does_not_return_the_host_path() {
    let source = RadosReplicaSource::with_identity(
        DataSourceId("source-a".to_string()),
        "inventory-a",
        PathBuf::from(r"D:\private\evidence\source.db"),
        ReplicaIdentity::from_inventory(Some(1), "uuid-inventory-a", Some("fsid-test".into())),
    )
    .unwrap();
    let mut provider = SourceDbRadosObjectProvider::new(
        vec![
            source,
            replica("source-b", "inventory-b"),
            replica("source-c", "inventory-c"),
        ],
        8,
        Vec::new(),
        3,
    )
    .unwrap();
    let request = RbdObjectReadRequest {
        object_no: 0,
        object_identity: "rbd_data.image-test.0000000000000000".to_string(),
        object_offset: 0,
        length: 4,
    };

    let error = provider
        .read_object_range(&request, &mut [0; 4])
        .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("source database could not be opened"));
    assert!(!message.contains("private"));
    assert!(!message.contains("evidence"));
}

#[test]
fn rejects_a_self_certified_single_replica_policy() {
    let error = SourceDbRadosObjectProvider::new(
        vec![replica("source-a", "inventory-a")],
        8,
        Vec::new(),
        1,
    )
    .err()
    .expect("single-replica policy must not self-certify");

    assert!(matches!(error, RadosProviderError::CoverageNotClosed));
}

#[test]
fn rejects_conflicting_replica_identity() {
    let mut replicas = vec![
        replica("source-a", "inventory-a"),
        replica("source-b", "inventory-b"),
        replica("source-c", "inventory-c"),
    ];
    for (index, item) in replicas.iter_mut().enumerate() {
        item.identity = ReplicaIdentity::from_inventory(
            Some(index as u32),
            format!("osd-{index}"),
            Some(if index == 1 { "fsid-b" } else { "fsid-a" }.to_string()),
        );
    }

    let error = match SourceDbRadosObjectProvider::new(replicas, 8, Vec::new(), 3) {
        Ok(_) => panic!("conflicting identity must fail closed"),
        Err(error) => error,
    };
    assert!(matches!(error, RadosProviderError::IdentityConflict { .. }));
}

#[test]
fn trusted_pool_policy_allows_only_the_proven_pool_size() {
    let policy = RbdReplicaPolicy::trusted_pool(8, 2, 1, "osdmap", Some(11), None)
        .expect("valid pool policy");
    let provider = SourceDbRadosObjectProvider::new_with_policy(
        vec![
            replica("source-a", "inventory-a"),
            replica("source-b", "inventory-b"),
        ],
        8,
        Vec::new(),
        policy,
    )
    .expect("two replicas match the trusted pool size");
    assert_eq!(provider.expected_replica_count, 2);

    let error = SourceDbRadosObjectProvider::new_with_policy(
        vec![replica("source-a", "inventory-a")],
        8,
        Vec::new(),
        RbdReplicaPolicy::trusted_pool(8, 2, 1, "osdmap", Some(11), None).unwrap(),
    )
    .err()
    .expect("missing a proven replica must fail closed");
    assert!(matches!(error, RadosProviderError::CoverageNotClosed));
}

#[test]
fn trusted_pool_policy_cannot_be_reused_for_another_data_pool() {
    let error = SourceDbRadosObjectProvider::new_with_policy(
        vec![
            replica("source-a", "inventory-a"),
            replica("source-b", "inventory-b"),
        ],
        9,
        Vec::new(),
        RbdReplicaPolicy::trusted_pool(8, 2, 1, "osdmap", Some(11), None).unwrap(),
    )
    .err()
    .expect("pool mismatch must fail closed");
    assert!(matches!(error, RadosProviderError::InvalidReplicaPolicy));
}

fn replica(data_source_id: &str, inventory_id: &str) -> RadosReplicaSource {
    RadosReplicaSource::with_identity(
        DataSourceId(data_source_id.to_string()),
        inventory_id,
        PathBuf::from(format!("sources/{data_source_id}/source.db")),
        ReplicaIdentity::from_inventory(
            Some(data_source_id.bytes().map(u32::from).sum()),
            format!("uuid-{inventory_id}"),
            Some("fsid-test".to_string()),
        ),
    )
    .unwrap()
}

#[test]
fn verified_object_cache_is_bounded_and_uses_lru_order() {
    let mut cache = VerifiedObjectCache::new(8, 2);
    cache.insert(
        "object-a",
        0,
        VerifiedObject::Present(Arc::from(vec![1, 2, 3, 4])),
    );
    cache.insert(
        "object-b",
        0,
        VerifiedObject::Present(Arc::from(vec![5, 6, 7, 8])),
    );
    assert!(cache.get("object-a", 0).is_some());

    cache.insert(
        "object-c",
        0,
        VerifiedObject::Present(Arc::from(vec![9, 10, 11, 12])),
    );

    assert!(cache.get("object-a", 0).is_some());
    assert!(cache.get("object-b", 0).is_none());
    assert!(cache.get("object-c", 0).is_some());
}

#[test]
fn verified_object_range_copy_is_exact_and_bounded() {
    let verified = VerifiedObject::Present(Arc::from(vec![10, 20, 30, 40, 50]));
    let request = RbdObjectReadRequest {
        object_no: 0,
        object_identity: "object-a".to_string(),
        object_offset: 1,
        length: 3,
    };
    let mut output = [0; 3];

    let outcome = copy_verified_segment(&request, 0, &mut output, &verified).expect("copy range");

    assert_eq!(output, [20, 30, 40]);
    assert!(matches!(
        outcome,
        RbdObjectReadOutcome::Present { bytes_read: 3, .. }
    ));
}

#[test]
fn verified_missing_object_range_preserves_missing_outcome() {
    let request = RbdObjectReadRequest {
        object_no: 0,
        object_identity: "object-a".to_string(),
        object_offset: 0,
        length: 4,
    };
    let mut output = [0xAA; 4];

    let outcome =
        copy_verified_segment(&request, 0, &mut output, &VerifiedObject::Missing).unwrap();

    assert_eq!(outcome, RbdObjectReadOutcome::Missing);
    assert_eq!(output, [0xAA; 4]);
}

#[test]
fn one_mib_request_coalesces_four_uncached_pages_per_device_read() {
    let cache = VerifiedObjectCache::new(8 * PAGE_BYTES, 8);
    let request = RbdObjectReadRequest {
        object_no: 0,
        object_identity: "object-a".to_string(),
        object_offset: 0,
        length: 1024 * 1024,
    };

    assert_eq!(
        range::coalesced_page_count(&cache, &request, 0, request.length as u64),
        4
    );
}

#[test]
fn sixty_four_kib_request_does_not_overread_adjacent_pages() {
    let cache = VerifiedObjectCache::new(8 * PAGE_BYTES, 8);
    let request = RbdObjectReadRequest {
        object_no: 0,
        object_identity: "object-a".to_string(),
        object_offset: 0,
        length: PAGE_BYTES,
    };

    assert_eq!(
        range::coalesced_page_count(&cache, &request, 0, request.length as u64),
        1
    );
}

#[test]
fn coalescing_stops_before_an_already_cached_page() {
    let mut cache = VerifiedObjectCache::new(8 * PAGE_BYTES, 8);
    cache.insert(
        "object-a",
        PAGE_BYTES as u64,
        VerifiedObject::Present(Arc::from(vec![1; PAGE_BYTES])),
    );
    let request = RbdObjectReadRequest {
        object_no: 0,
        object_identity: "object-a".to_string(),
        object_offset: 0,
        length: 1024 * 1024,
    };

    assert_eq!(
        range::coalesced_page_count(&cache, &request, 0, request.length as u64),
        1
    );
}
