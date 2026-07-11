use transport::commands::ClassifyFilesRequest;

#[test]
fn classify_files_request_deserializes_sample_size() {
    let request: ClassifyFilesRequest =
        serde_json::from_str(r#"{"dataSourceId":"ds-1","sampleSize":1000}"#)
            .expect("classification request must deserialize");

    assert_eq!(request.data_source_id, "ds-1");
    assert_eq!(request.sample_size, Some(1000));
}
