use super::*;

#[test]
fn payload_deserializes_with_camel_case() {
    let json = r#"{"caseId":"case-123","dataSourceId":"ds-456","name":"C","kind":"E01","jobId":"job-789"}"#;
    let payload: DataSourceImportedPayload = serde_json::from_str(json).unwrap();
    assert_eq!(payload.case_id, "case-123");
}
