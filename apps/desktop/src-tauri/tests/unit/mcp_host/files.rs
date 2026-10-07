use super::{request, server};
use crate::state::AppState;
use app_services::{
    case_service, file_service,
    source_db::{self, GlobalFileId},
};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};
use serde_json::{json, Value};
use transport::dto::mcp_host::McpHostSettingsDto;

fn attach_evidence(state: &AppState, temporary: &tempfile::TempDir) -> String {
    let evidence = temporary.path().join("evidence");
    std::fs::create_dir(&evidence).unwrap();
    std::fs::write(evidence.join("proof.txt"), b"MCP byte proof").unwrap();
    let case =
        case_service::create_case(&temporary.path().join("cases"), "HTTP File Case", None).unwrap();
    let source = DataSource {
        id: DataSourceId("mcp-http-files".into()),
        name: "evidence".into(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: evidence,
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source.id.0, Some("windows"), None);
    storage.import_state = "ready".into();
    DataSourceRepo::new(&case.connection().unwrap())
        .insert_with_storage(&case.meta.id, &source, &storage)
        .unwrap();
    let conn = source_db::open_source_db(&case.case_root, &source.id).unwrap();
    DataSourceRepo::new(&conn)
        .upsert_source_local_metadata(&case.meta.id, &source)
        .unwrap();
    let fs = evidence_core::LogicalFsReader::open(&source.source_path, "evidence").unwrap();
    file_service::enumerate_filesystem(&conn, &source.id, &fs).unwrap();
    let entry = FileRepo::new(&conn)
        .find_by_data_source(&source.id)
        .unwrap()
        .into_iter()
        .find(|entry| entry.name == "proof.txt")
        .unwrap();
    let file_id = GlobalFileId::new(source.id, entry.id).encode().0;
    *state.active_case.lock().unwrap() = Some(case);
    file_id
}

async fn call(endpoint: &str, tool: &str, arguments: Value) -> Value {
    let response = request(endpoint, json!({"jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{"name":tool,"arguments":arguments}})).await;
    assert_eq!(response.status(), 200);
    response.json::<Value>().await.unwrap()["result"].clone()
}

#[tokio::test]
async fn http_files_return_evidence_bytes_audit_calls_and_enforce_policy_and_case_switch() {
    let (state, endpoint, temporary) = server().await;
    let file_id = attach_evidence(&state, &temporary);
    let initialize = request(&endpoint, json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"file-regression","version":"1"}}})).await;
    assert_eq!(initialize.status(), 200);
    let roots = call(
        &endpoint,
        "forensics.list_files",
        json!({"dataSourceId":"mcp-http-files"}),
    )
    .await;
    assert_eq!(roots["isError"], false);
    let directory = roots["structuredContent"]["rows"][0]["id"].clone();
    let children = call(
        &endpoint,
        "forensics.list_files",
        json!({"dataSourceId":"mcp-http-files", "parentId":directory}),
    )
    .await;
    assert_eq!(children["structuredContent"]["rows"][0]["id"], file_id);
    let metadata = call(
        &endpoint,
        "forensics.get_file_metadata",
        json!({"fileId":file_id}),
    )
    .await;
    assert_eq!(metadata["structuredContent"]["size"], 14);
    let content = call(&endpoint, "forensics.read_file", json!({"fileId":file_id})).await;
    assert_eq!(content["isError"], false);
    assert_eq!(content["structuredContent"]["content"], "MCP byte proof");
    assert_eq!(content["structuredContent"]["eof"], true);
    assert_eq!(state.preview_runtime.stats().unwrap().session_count, 0);
    let bad = call(
        &endpoint,
        "forensics.read_file",
        json!({"fileId":file_id, "path":"C:\\private.txt"}),
    )
    .await;
    assert_eq!(bad["isError"], true);
    assert!(!bad.to_string().contains("private.txt"));
    state
        .mcp_host
        .configure(
            state.clone(),
            McpHostSettingsDto {
                enabled: true,
                disabled_tools: vec!["forensics.read_file".into()],
            },
        )
        .await
        .unwrap();
    let blocked = call(&endpoint, "forensics.read_file", json!({"fileId":file_id})).await;
    assert_eq!(blocked["isError"], true);
    let conn = state.get_connection().unwrap();
    let count = persistence_sqlite::repositories::audit_repo::AuditRepo::new(&conn)
        .count_by_action("mcp.tool.call")
        .unwrap();
    assert_eq!(count, 6);
    state
        .mcp_host
        .configure(state.clone(), McpHostSettingsDto::default())
        .await
        .unwrap();
    *state.active_case.lock().unwrap() = Some(
        case_service::create_case(&temporary.path().join("cases"), "Other Case", None).unwrap(),
    );
    let stale = call(&endpoint, "forensics.read_file", json!({"fileId":file_id})).await;
    assert_eq!(stale["isError"], true);
    assert!(!stale.to_string().contains("MCP byte proof"));
    assert_eq!(
        std::fs::read(temporary.path().join("evidence/proof.txt")).unwrap(),
        b"MCP byte proof"
    );
    state.mcp_host.shutdown();
}
