use app_services::{
    analysis_service::AnalysisSourceReadRuntime,
    case_service,
    mcp_host_service::{
        execute_analysis_tool, execute_tool, tool_catalog, validate_settings,
        McpHostAnalysisQueryContext, McpHostQueryContext, McpHostServiceError,
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
    assert_eq!(catalog.len(), 28);
    for name in [
        "forensics.get_analysis_system_info",
        "forensics.get_file_classification_board",
        "forensics.get_event_log_events",
        "forensics.get_plugin_family_entries",
        "forensics.get_analysis_report",
        "forensics.get_case_overview",
        "forensics.get_correlation_snapshot",
        "forensics.get_timeline_events",
        "forensics.get_timeline_event",
        "forensics.get_timeline_facets",
        "forensics.search_files",
        "forensics.get_content_index_coverage",
    ] {
        assert!(catalog.iter().any(|tool| tool.name == name), "{name}");
    }
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

#[test]
fn timeline_search_and_empty_case_analysis_use_real_read_only_services() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "MCP Read Models", None).unwrap();
    let conn = case.connection().unwrap();
    let context = || McpHostQueryContext {
        connection: &conn,
        case_root: &case.case_root,
        case_meta: &case.meta,
    };

    let timeline = execute_tool(
        context(),
        "forensics.get_timeline_events",
        &json!({"limit":25}),
    )
    .unwrap();
    assert_eq!(timeline["total"], 0);
    let facets = execute_tool(
        context(),
        "forensics.get_timeline_facets",
        &json!({"bucketCount":20}),
    )
    .unwrap();
    assert_eq!(facets["totalEvents"], 0);
    let search = execute_tool(
        context(),
        "forensics.search_files",
        &json!({"query":"","entryType":"file","dataSourceIds":[]}),
    )
    .unwrap();
    assert_eq!(search["total"], 0);
    assert_eq!(search["coverage"]["complete"], true);

    let runtime = AnalysisSourceReadRuntime::default();
    let analysis_context = || McpHostAnalysisQueryContext {
        query: context(),
        runtime: &runtime,
    };
    assert!(execute_analysis_tool(
        analysis_context(),
        "forensics.get_case_overview",
        &json!({})
    )
    .is_ok());
    assert!(execute_analysis_tool(
        analysis_context(),
        "forensics.get_correlation_snapshot",
        &json!({})
    )
    .is_ok());
}

#[test]
fn source_analysis_routes_share_validation_without_accepting_host_paths() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "MCP Analysis Routes", None).unwrap();
    let conn = case.connection().unwrap();
    let runtime = AnalysisSourceReadRuntime::default();
    let context = || McpHostAnalysisQueryContext {
        query: McpHostQueryContext {
            connection: &conn,
            case_root: &case.case_root,
            case_meta: &case.meta,
        },
        runtime: &runtime,
    };
    let source_tools = [
        "forensics.get_analysis_system_info",
        "forensics.get_evidence_classification_summary",
        "forensics.get_file_classification_board",
        "forensics.get_registry_summary",
        "forensics.get_registry_structured_summary",
        "forensics.get_browser_history_summary",
        "forensics.get_email_summary",
        "forensics.get_event_log_events",
        "forensics.get_linux_artifact_summary",
        "forensics.get_android_device_info",
        "forensics.get_android_package_summary",
        "forensics.get_analysis_report",
    ];
    for name in source_tools {
        let result =
            execute_analysis_tool(context(), name, &json!({"dataSourceId":"outside-case"}));
        assert!(
            matches!(result, Err(McpHostServiceError::Analysis(_))),
            "{name}"
        );
    }
    let invalid = execute_analysis_tool(
        context(),
        "forensics.get_analysis_system_info",
        &json!({"dataSourceId":"outside-case","hostPath":"C:\\private"}),
    );
    assert!(matches!(invalid, Err(McpHostServiceError::InvalidInput)));
}
