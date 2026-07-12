use super::*;

#[test]
fn registry_transaction_dto_serializes_as_camel_case() {
    let dto = RegistryTransactionDto {
        operation: RegistryTransactionOperationDto::SetValue,
        key_path: "\\Registry\\Machine\\SOFTWARE\\Test".to_string(),
        value_name: Some("KeyName".to_string()),
        data_before: Some("aGV4".to_string()),
        data_after: Some("d29ybGQ=".to_string()),
        sequence_number: 42,
        timestamp: Some("2026-06-14T12:00:00Z".to_string()),
    };

    let value = serde_json::to_value(&dto).unwrap();
    assert_eq!(value["operation"], "setValue");
    assert_eq!(value["keyPath"], "\\Registry\\Machine\\SOFTWARE\\Test");
    assert_eq!(value["valueName"], "KeyName");
    assert_eq!(value["sequenceNumber"], 42);
    assert_eq!(value["timestamp"], "2026-06-14T12:00:00Z");
    // Check that snake_case keys are absent.
    assert!(value.get("key_path").is_none());
    assert!(value.get("value_name").is_none());
    assert!(value.get("sequence_number").is_none());
}

#[test]
fn registry_transaction_dto_skips_optional_fields() {
    let dto = RegistryTransactionDto {
        operation: RegistryTransactionOperationDto::CreateKey,
        key_path: "\\Key".to_string(),
        value_name: None,
        data_before: None,
        data_after: None,
        sequence_number: 1,
        timestamp: None,
    };

    let value = serde_json::to_value(&dto).unwrap();
    assert!(value.get("valueName").is_none());
    assert!(value.get("dataBefore").is_none());
    assert!(value.get("dataAfter").is_none());
    assert!(value.get("timestamp").is_none());
}
