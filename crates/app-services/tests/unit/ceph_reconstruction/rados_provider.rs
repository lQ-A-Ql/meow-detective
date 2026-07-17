use std::path::PathBuf;

use domain::DataSourceId;

use super::cache::{copy_verified_segment, VerifiedObject, VerifiedObjectCache, PAGE_BYTES};
use super::*;

#[test]
fn rejects_empty_replica_coverage() {
    let error = SourceDbRadosObjectProvider::new(Vec::new(), 8, Vec::new(), 0)
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

    let error = SourceDbRadosObjectProvider::new(vec![first, second], 8, Vec::new(), 2)
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

    let error = SourceDbRadosObjectProvider::new(vec![first, second], 8, Vec::new(), 2)
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

    let error = SourceDbRadosObjectProvider::new(vec![source], 8, Vec::new(), 2)
        .err()
        .expect("incomplete coverage should fail");

    assert!(matches!(error, RadosProviderError::CoverageNotClosed));
}

#[test]
fn source_db_open_failure_does_not_return_the_host_path() {
    let source = RadosReplicaSource::new(
        DataSourceId("source-a".to_string()),
        "inventory-a",
        PathBuf::from(r"D:\private\evidence\source.db"),
    )
    .unwrap();
    let mut provider = SourceDbRadosObjectProvider::new(vec![source], 8, Vec::new(), 1).unwrap();
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
