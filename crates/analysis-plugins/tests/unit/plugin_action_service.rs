//! Unit tests for the generic plugin action channel service.

use super::*;

#[test]
fn blank_plugin_id_is_invalid_input() {
    let error = list_plugin_actions("  ").expect_err("blank id");
    assert!(matches!(error, PluginActionError::InvalidInput(_)));
}

#[test]
fn unknown_plugin_is_not_found() {
    let error = list_plugin_actions("meow.plugin.does-not-exist").expect_err("unknown plugin");
    assert!(matches!(error, PluginActionError::NotFound("Plugin", _)));
}

#[test]
fn descriptor_parsing_is_tolerant() {
    let valid = serde_json::json!({
        "id": "recoverKeys",
        "label": "从内存镜像恢复数据库密钥",
        "description": "扫描并验证",
        "inputKind": "file"
    });
    let descriptor = parse_descriptor(&valid).expect("valid descriptor");
    assert_eq!(descriptor.id, "recoverKeys");
    assert_eq!(descriptor.input_kind, "file");
    assert_eq!(descriptor.description.as_deref(), Some("扫描并验证"));

    // inputKind defaults to "none"; missing id/label drops the entry.
    let minimal = serde_json::json!({ "id": "x", "label": "y" });
    assert_eq!(
        parse_descriptor(&minimal).expect("minimal").input_kind,
        "none"
    );
    assert!(parse_descriptor(&serde_json::json!({ "label": "y" })).is_none());
    assert!(parse_descriptor(&serde_json::json!({ "id": "x" })).is_none());
}
