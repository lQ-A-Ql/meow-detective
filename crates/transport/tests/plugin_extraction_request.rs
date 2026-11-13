use transport::commands::RunPluginExtractionRequest;

#[test]
fn plugin_extraction_request_roundtrips_source_identity() {
    let request: RunPluginExtractionRequest =
        serde_json::from_str(r#"{"dataSourceId":"source-1"}"#).unwrap();
    request.validate().unwrap();
    assert_eq!(request.data_source_id, "source-1");
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        serde_json::json!({ "dataSourceId": "source-1" })
    );
}

#[test]
fn plugin_extraction_request_rejects_native_categories_and_browser_passwords() {
    for value in [
        serde_json::json!({ "dataSourceId": "source-1", "categories": ["Registry"] }),
        serde_json::json!({ "dataSourceId": "source-1", "browserPassword": "secret" }),
    ] {
        assert!(serde_json::from_value::<RunPluginExtractionRequest>(value).is_err());
    }
}

#[test]
fn plugin_extraction_request_requires_a_source() {
    let request = RunPluginExtractionRequest { data_source_id: " ".into() };
    assert!(request.validate().is_err());
}
