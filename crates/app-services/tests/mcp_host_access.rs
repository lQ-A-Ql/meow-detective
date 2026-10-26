use app_services::mcp_host_service::access::McpAccessRegistry;
use transport::dto::mcp_host::access::McpHostCreateClientRequestDto;

fn request(name: &str, hours: u32) -> McpHostCreateClientRequestDto {
    McpHostCreateClientRequestDto {
        name: name.into(),
        expires_after_hours: hours,
        read_evidence: false,
    }
}

#[test]
fn credentials_are_unique_bounded_and_revocation_invalidates_existing_principals() {
    let mut registry = McpAccessRegistry::default();
    for (name, hours) in [("", 24), ("reader", 0), ("reader", 721), ("reader\n", 24)] {
        assert!(registry
            .prepare(request(name, hours), "case", "session")
            .is_err());
    }
    let (first, record) = registry
        .prepare(request("reader", 1), "case", "session")
        .unwrap();
    let (second, _) = registry
        .prepare(request("reader", 1), "case", "session")
        .unwrap();
    assert_ne!(first.token, second.token);
    assert_ne!(first.client.id, second.client.id);
    assert!(McpAccessRegistry::load(vec![record.clone(), record.clone()]).is_err());
    registry.insert(record);
    let principal = registry.authenticate(&first.token).unwrap();
    let catalog = app_services::mcp_host_service::tool_catalog();
    let allowed =
        |name: &str| principal.allows(catalog.iter().find(|tool| tool.name == name).unwrap());
    assert!(allowed("forensics.get_current_case"));
    assert!(!allowed("forensics.read_file"));
    assert!(!allowed("forensics.extract_file"));
    assert!(registry.authenticate(&second.token).is_err());
    registry.revoke(&first.client.id);
    assert!(!principal.active());
    assert!(registry.authenticate(&first.token).is_err());
    assert!(registry.records().is_empty());
    assert!(registry.clients().is_empty());
}
