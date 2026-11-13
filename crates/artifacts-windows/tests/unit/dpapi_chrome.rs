use super::*;
use serde_json::json;

#[test]
fn missing_optional_app_bound_material_does_not_report_a_key_error() {
    let root = json!({"os_crypt": {}});
    assert!(unwrap_optional_app_bound_material(
        &root,
        &HashMap::new(),
        ChromiumFamily::Chrome,
        None,
        None,
    )
    .unwrap()
    .is_none());
}

#[test]
fn present_invalid_app_bound_material_remains_an_error() {
    for value in [
        Value::Null,
        json!(false),
        json!(7),
        json!("!"),
        json!("AA=="),
    ] {
        let root = json!({"os_crypt": {"app_bound_encrypted_key": value}});
        assert!(unwrap_optional_app_bound_material(
            &root,
            &HashMap::new(),
            ChromiumFamily::Chrome,
            None,
            None,
        )
        .is_err());
    }
}
