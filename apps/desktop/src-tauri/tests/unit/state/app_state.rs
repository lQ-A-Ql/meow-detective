use super::*;
use mcp_client::{McpPermissionProfile, McpTransport};

fn test_server(id: &str) -> McpServerConfig {
    McpServerConfig {
        id: id.to_string(),
        name: format!("Server {id}"),
        transport: McpTransport::Sse {
            url: "http://localhost:3001/sse".to_string(),
        },
        enabled: true,
        auto_connect: false,
        permissions: McpPermissionProfile::default(),
    }
}

#[tokio::test]
async fn sync_mcp_clients_with_config_removes_stale_clients() {
    let state = AppState::default();
    {
        let mut config = state.mcp_config.lock().unwrap();
        config.servers = vec![test_server("keep")];
    }

    state
        .replace_mcp_client("keep".to_string(), McpClient::new(test_server("keep")))
        .await
        .unwrap();
    state
        .replace_mcp_client("drop".to_string(), McpClient::new(test_server("drop")))
        .await
        .unwrap();

    state.sync_mcp_clients_with_config().await.unwrap();

    assert!(state.get_mcp_client("keep").await.is_ok());
    assert!(state.get_mcp_client("drop").await.is_err());
}
