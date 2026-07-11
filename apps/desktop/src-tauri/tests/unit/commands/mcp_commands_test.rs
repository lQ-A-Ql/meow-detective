use super::mapping::{test_transport_summary_from_request, transport_from_dto};
use transport::dto::mcp::{
    McpPermissionProfileDto, McpServerConfigDto, McpTestConnectionRequestDto,
};

fn dummy_permissions() -> McpPermissionProfileDto {
    McpPermissionProfileDto {
        resource_access: "readOnly".to_string(),
        tool_access: "disabled".to_string(),
        prompt_access: "readOnly".to_string(),
        network_policy: "localhostOnly".to_string(),
        allowed_tools: vec![],
        allowed_commands: vec![],
    }
}

#[test]
fn transport_from_dto_rejects_invalid_transport() {
    let server = McpServerConfigDto {
        id: "s1".to_string(),
        name: "Server".to_string(),
        transport_type: "invalid".to_string(),
        url: None,
        command: None,
        args: None,
        enabled: false,
        auto_connect: false,
        permissions: dummy_permissions(),
    };
    let error = transport_from_dto(&server).unwrap_err();
    assert!(error.to_string().contains("Invalid transport type"));
}

#[test]
fn test_transport_summary_rejects_invalid_transport() {
    let request = McpTestConnectionRequestDto {
        transport_type: "invalid".to_string(),
        url: None,
        command: None,
        args: None,
        permissions: dummy_permissions(),
    };
    let error = test_transport_summary_from_request(&request).unwrap_err();
    assert!(error.to_string().contains("Invalid transport type"));
}
