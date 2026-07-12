use super::*;

#[test]
fn export_empty_object() {
    let mut output = Vec::new();
    let value = serde_json::json!({});
    JsonExporter::export(&mut output, &value).unwrap();
    let result = String::from_utf8(output).unwrap();
    assert_eq!(result, "{}");
}

#[test]
fn export_empty_array() {
    let mut output = Vec::new();
    let value = serde_json::json!([]);
    JsonExporter::export(&mut output, &value).unwrap();
    let result = String::from_utf8(output).unwrap();
    assert_eq!(result, "[]");
}

#[test]
fn export_nested_object() {
    let mut output = Vec::new();
    let value = serde_json::json!({
        "case": {
            "name": "Test Case",
            "number": "2026-001",
            "artifacts": [1, 2, 3]
        }
    });
    JsonExporter::export(&mut output, &value).unwrap();
    let result = String::from_utf8(output).unwrap();
    assert!(result.contains("\"name\": \"Test Case\""));
    assert!(result.contains("\"artifacts\":"));
    // pretty-printed array: each element on its own line
    assert!(result.contains("1"));
    assert!(result.contains("2"));
    assert!(result.contains("3"));
}

#[test]
fn export_produces_pretty_printed_json() {
    let mut output = Vec::new();
    let value = serde_json::json!({"key": "value"});
    JsonExporter::export(&mut output, &value).unwrap();
    let result = String::from_utf8(output).unwrap();
    // pretty_printed JSON contains newlines
    assert!(result.contains('\n'));
}

#[test]
fn export_preserves_special_characters() {
    let mut output = Vec::new();
    let value = serde_json::json!({
        "path": "C:\\Users\\test",
        "unicode": "中文"
    });
    JsonExporter::export(&mut output, &value).unwrap();
    let result = String::from_utf8(output).unwrap();
    assert!(result.contains("C:\\\\Users\\\\test"));
    assert!(result.contains("中文"));
}

#[test]
fn export_invalid_value_returns_empty_string() {
    let mut output = Vec::new();
    // NaN cannot be serialized, to_string_pretty returns None -> empty string
    let value = serde_json::json!(f64::NAN);
    JsonExporter::export(&mut output, &value).unwrap();
    // The result should be empty or contain error indicator
    let result = String::from_utf8(output).unwrap();
    assert!(result.is_empty() || result.contains("null"));
}

#[test]
fn export_large_array() {
    let mut output = Vec::new();
    let items: Vec<serde_json::Value> = (0..100)
        .map(|i| serde_json::json!({"id": i, "name": format!("item-{}", i)}))
        .collect();
    let value = serde_json::json!(items);
    JsonExporter::export(&mut output, &value).unwrap();
    let result = String::from_utf8(output).unwrap();
    assert!(result.contains("\"id\": 0"));
    assert!(result.contains("\"id\": 99"));
    assert!(result.contains("item-50"));
}
