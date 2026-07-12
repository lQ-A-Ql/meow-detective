use super::*;

#[test]
fn parse_empty_data() {
    let result = parse_azure_activity_log("");
    assert!(result.is_err());
}

#[test]
fn parse_azure_json_array() {
    let json = r#"[
        {
            "authorization": {
                "action": "Microsoft.Storage/storageAccounts/read",
                "scope": "/subscriptions/sub-id/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/mystorage"
            },
            "caller": "alice@example.com",
            "eventTimestamp": "2024-06-15T12:00:00Z",
            "resourceId": "/subscriptions/sub-id/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/mystorage"
        },
        {
            "authorization": {
                "action": "Microsoft.Compute/virtualMachines/start/action"
            },
            "caller": "bob@example.com",
            "eventTimestamp": "2024-06-15T12:05:00Z",
            "resourceId": "/subscriptions/sub-id/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/myvm"
        }
    ]"#;

    let entries = parse_azure_activity_log(json).expect("should parse");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].action, "Microsoft.Storage/storageAccounts/read");
    assert_eq!(entries[0].principal.as_deref(), Some("alice@example.com"));
    assert!(entries[0].target.is_some());
    assert_eq!(
        entries[1].action,
        "Microsoft.Compute/virtualMachines/start/action"
    );
}

#[test]
fn parse_azure_single_object() {
    let json = r#"{
        "authorization": {"action": "Microsoft.Network/networkSecurityGroups/read"},
        "caller": "sp-xyz",
        "eventTimestamp": "2024-06-15T13:00:00Z",
        "resourceId": "/subscriptions/sub-id/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/nsg1"
    }"#;

    let entries = parse_azure_activity_log(json).expect("should parse");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].principal.as_deref(), Some("sp-xyz"));
}
