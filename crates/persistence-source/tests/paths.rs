#[test]
fn detail_storage_missing_message_is_stable() {
    assert_eq!(
        persistence_source::SourceError::DetailStorageMissing.to_string(),
        "source detail storage is missing"
    );
}
