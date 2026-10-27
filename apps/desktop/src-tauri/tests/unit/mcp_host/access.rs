use super::{grant, request, server};
use serde_json::{json, Value};
use transport::dto::mcp_host::access::McpHostCredentialRecordDto;

#[tokio::test]
async fn http_credentials_enforce_scope_expiry_revocation_and_case_session() {
    let (state, endpoint, temporary) = server().await;
    let case =
        app_services::case_service::create_case(temporary.path(), "Authorized case", None).unwrap();
    let case_root = case.case_root.clone();
    *state.active_case.lock().unwrap() = Some(case);
    let created = grant(&state, false);
    let list = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
    let missing = reqwest::Client::new()
        .post(&endpoint)
        .json(&list)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 401);
    let invalid = request(&endpoint, "invalid", list.clone()).await;
    assert_eq!(invalid.status(), 401);
    assert_eq!(invalid.headers()["www-authenticate"], "Bearer");
    let response: Value = request(&endpoint, &created.token, list.clone())
        .await
        .json()
        .await
        .unwrap();
    let tools = response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 28);
    assert!(!tools
        .iter()
        .any(|tool| tool["name"] == "forensics.read_file"));
    let result: Value = request(&endpoint, &created.token, json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"forensics.read_file","arguments":{"fileId":"unknown"}}
    })).await.json().await.unwrap();
    assert_eq!(result["result"]["isError"], true);
    assert_eq!(
        result["result"]["structuredContent"]["error"]["category"],
        "security"
    );
    assert_eq!(state.preview_runtime.stats().unwrap().session_count, 0);
    state
        .mcp_host
        .revoke_client(&state, &created.client.id)
        .unwrap();
    assert_eq!(
        request(&endpoint, &created.token, list.clone())
            .await
            .status(),
        401
    );
    let new = grant(&state, true);
    let reopened = app_services::case_service::open_case(&case_root).unwrap();
    *state.active_case.lock().unwrap() = Some(reopened);
    assert_eq!(request(&endpoint, &new.token, list).await.status(), 403);
    state.mcp_host.shutdown();
}

#[tokio::test]
async fn credential_storage_contains_digests_and_expired_records_cannot_authenticate() {
    let (state, endpoint, temporary) = server().await;
    *state.active_case.lock().unwrap() = Some(
        app_services::case_service::create_case(temporary.path(), "Credential storage", None)
            .unwrap(),
    );
    let created = grant(&state, true);
    let path = temporary.path().join("mcp-access.json");
    let bytes = std::fs::read(&path).unwrap();
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains(&created.token));
    let mut records: Vec<McpHostCredentialRecordDto> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(records[0].client, created.client);
    assert!(records[0]
        .export_grants
        .iter()
        .all(|grant| !grant.root_path.is_empty()));
    let connection = state.get_connection().unwrap();
    let events = persistence_sqlite::repositories::audit_repo::AuditRepo::new(&connection)
        .query(None, Some("mcp.access.grant"), 10, 0)
        .unwrap();
    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .all(|event| !event.details.contains(&created.token)));
    let reloaded = crate::mcp_host::McpHostRuntime::default();
    reloaded.load_access(&state).unwrap();
    assert!(reloaded
        .access
        .lock()
        .unwrap()
        .authenticate(&created.token)
        .is_ok());
    records[0].client.expires_at = chrono::Utc::now().timestamp() - 1;
    let expired = app_services::mcp_host_service::access::McpAccessRegistry::load(records).unwrap();
    assert!(expired.authenticate(&created.token).is_err());
    std::fs::write(&path, serde_json::to_vec(&expired.records()).unwrap()).unwrap();
    state.mcp_host.load_access(&state).unwrap();
    assert_eq!(
        request(
            &endpoint,
            &created.token,
            json!({"jsonrpc":"2.0","id":1,"method":"ping"})
        )
        .await
        .status(),
        401
    );
    std::fs::write(&path, b"invalid-json").unwrap();
    assert!(reloaded.load_access(&state).is_err());
    state.mcp_host.initialize(state.clone()).await;
    assert!(!state.mcp_host.status().unwrap().running);
    assert!(!state.mcp_host.status().unwrap().settings.enabled);
    state.mcp_host.shutdown();
}
