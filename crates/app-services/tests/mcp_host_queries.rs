use app_services::{
    case_service,
    mcp_host_service::{
        execute_tool, tool_catalog, validate_settings, McpHostQueryContext, McpHostServiceError,
    },
};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use serde_json::json;
use transport::dto::mcp_host::McpHostSettingsDto;

#[test]
fn current_case_sources_and_details_share_real_service_queries_with_no_host_path() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "MCP Query", None).unwrap();
    let conn = case.connection().unwrap();
    let source = DataSource {
        id: DataSourceId("source-mcp".into()),
        name: "Evidence".into(),
        kind: DataSourceKind::E01,
        source_path: temporary.path().join("private-evidence.E01"),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source.id.0, Some("windows"), None);
    storage.last_error = Some("C:\\private\\secret.E01".into());
    DataSourceRepo::new(&conn)
        .insert_with_storage(&case.meta.id, &source, &storage)
        .unwrap();
    let context = || McpHostQueryContext {
        connection: &conn,
        case_root: &case.case_root,
        case_meta: &case.meta,
    };
    let current = execute_tool(context(), "forensics.get_current_case", &json!({})).unwrap();
    assert_eq!(current["id"], case.meta.id.0);
    let sources = execute_tool(context(), "forensics.list_data_sources", &json!({})).unwrap();
    assert_eq!(sources[0]["id"], "source-mcp");
    assert_eq!(sources[0]["importState"], "pending");
    assert!(sources[0].get("sourcePath").is_none());
    assert!(!sources.to_string().contains("private"));
    let detail = execute_tool(
        context(),
        "forensics.get_data_source",
        &json!({"dataSourceId":"source-mcp"}),
    )
    .unwrap();
    assert_eq!(detail, sources[0]);
    assert!(matches!(
        execute_tool(
            context(),
            "forensics.get_data_source",
            &json!({"dataSourceId":"outside-case"})
        ),
        Err(McpHostServiceError::NotFound)
    ));
    for (name, args) in [
        ("forensics.list_plugin_modules", json!({})),
        (
            "forensics.get_plugin_module",
            json!({"dataSourceId":"source-mcp"}),
        ),
        (
            "forensics.get_current_case",
            json!({"caseRoot":"arbitrary"}),
        ),
        ("forensics.get_data_source", json!({"dataSourceId":[] })),
    ] {
        assert!(matches!(
            execute_tool(context(), name, &args),
            Err(McpHostServiceError::InvalidInput)
        ));
    }
    let error = execute_tool(
        context(),
        "forensics.list_plugin_modules",
        &json!({"dataSourceId":"outside-case"}),
    )
    .unwrap_err();
    assert!(!error.to_string().contains("outside-case"));
}

#[test]
fn host_catalog_is_complete_and_policy_validation_rejects_unknown_tools() {
    let catalog = tool_catalog();
    assert_eq!(catalog.len(), 8);
    for tool in &catalog {
        assert_eq!(tool.input_schema["additionalProperties"], false);
    }
    let mut settings = McpHostSettingsDto {
        enabled: true,
        disabled_tools: vec![catalog[0].name.clone(), catalog[0].name.clone()],
    };
    validate_settings(&mut settings).unwrap();
    assert_eq!(settings.disabled_tools.len(), 1);
    settings
        .disabled_tools
        .push("forensics.arbitrary_execute".into());
    assert!(validate_settings(&mut settings).is_err());
}
