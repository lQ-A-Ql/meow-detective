use super::tool_response;

#[test]
fn result_budget_preserves_data_and_rejects_oversized_serialized_content() {
    let data = serde_json::json!({"id":"case-id", "message":"查询完成"});
    let accepted = tool_response(Ok(data.clone()));
    assert!(accepted.success);
    assert_eq!(accepted.data, Some(data));
    let rejected = tool_response(Ok(serde_json::json!({"body":"x".repeat(2 * 1024 * 1024)})));
    assert!(!rejected.success);
    assert!(rejected.data.is_none());
    assert_eq!(rejected.error_details.unwrap().code, "MCP_RESULT_TOO_LARGE");
}
