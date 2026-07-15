use std::path::PathBuf;

use domain::DataSourceId;

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
