use app_services::{
    analysis_service::AnalysisSourceReadRuntime,
    case_service,
    mcp_host_service::{
        execute_analysis_tool, execute_tool, tool_catalog, validate_settings,
        McpHostAnalysisQueryContext, McpHostQueryContext, McpHostServiceError,
    },
};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    job_repo::JobRepo,
};
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
    let missing = execute_tool(
        context(),
        "forensics.get_data_source",
        &json!({"dataSourceId":"outside-case"}),
    )
    .unwrap_err();
    assert!(matches!(missing, McpHostServiceError::NotFoundDetail(_)));
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
            Err(McpHostServiceError::Invalid(_))
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
    assert_eq!(catalog.len(), 51);
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
        "forensics.extract_file",
        "forensics.browse_registry_key",
        "forensics.query_graph",
        "forensics.get_provenance_chain",
        "forensics.list_jobs",
        "forensics.get_job",
        "forensics.get_task",
        "forensics.cancel_job",
        "forensics.start_content_index",
        "forensics.start_analysis",
        "forensics.list_report_templates",
        "forensics.get_report_history",
        "forensics.start_report_export",
        "forensics.get_mcp_metrics",
        "forensics.list_linux_services",
        "forensics.list_docker_containers",
        "forensics.list_docker_images",
    ] {
        assert!(catalog.iter().any(|tool| tool.name == name), "{name}");
    }
    for tool in &catalog {
        assert_eq!(tool.input_schema["additionalProperties"], false);
    }
    let jobs = catalog
        .iter()
        .find(|tool| tool.name == "forensics.list_jobs")
        .unwrap();
    assert_eq!(jobs.output_schema["type"], "array");
    assert_eq!(
        catalog
            .iter()
            .find(|tool| tool.name == "forensics.get_job")
            .unwrap()
            .output_schema["type"],
        "object"
    );
    assert_eq!(
        catalog
            .iter()
            .find(|tool| tool.name == "forensics.get_mcp_metrics")
            .unwrap()
            .output_schema["properties"]["peakRssMb"]["type"],
        "integer"
    );
    assert_eq!(
        catalog
            .iter()
            .find(|tool| tool.name == "forensics.read_file")
            .unwrap()
            .output_schema["properties"]["chunkSha256"]["minLength"],
        64
    );
    let extractor = catalog
        .iter()
        .find(|tool| tool.name == "forensics.extract_file")
        .unwrap();
    assert!(!extractor.read_only);
    assert!(extractor.destructive);
    let reader = catalog
        .iter()
        .find(|tool| tool.name == "forensics.read_file")
        .unwrap();
    assert!(reader.read_only);
    assert!(!reader.destructive);
    let mut settings = McpHostSettingsDto {
        enabled: true,
        disabled_tools: vec![catalog[0].name.clone(), catalog[0].name.clone()],
        policy_version: 1,
    };
    validate_settings(&mut settings).unwrap();
    assert_eq!(settings.disabled_tools.len(), 1);
    settings
        .disabled_tools
        .push("forensics.arbitrary_execute".into());
    assert!(validate_settings(&mut settings).is_err());
}

#[test]
fn job_queries_read_exact_case_job_and_redact_host_detail() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "MCP Jobs", None).unwrap();
    let conn = case.connection().unwrap();
    let job_id = JobRepo::new(&conn)
        .create(&case.meta.id.0, "Content index")
        .unwrap();
    JobRepo::new(&conn)
        .update_progress(&job_id, 45, r"reading C:\private\evidence")
        .unwrap();
    let context = || McpHostQueryContext {
        connection: &conn,
        case_root: &case.case_root,
        case_meta: &case.meta,
    };

    let list = execute_tool(context(), "forensics.list_jobs", &json!({})).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], job_id.0);
    assert_eq!(list[0]["progress"], 45);
    assert_eq!(list[0]["detail"], "任务详情包含受保护路径");

    let detail = execute_tool(context(), "forensics.get_job", &json!({"jobId":job_id.0})).unwrap();
    assert_eq!(detail["id"], job_id.0);
    assert_eq!(detail["detail"], "任务详情包含受保护路径");
    assert!(matches!(
        execute_tool(context(), "forensics.get_job", &json!({"jobId":"missing"})),
        Err(McpHostServiceError::NotFound)
    ));
    let task_error = execute_tool(
        context(),
        "forensics.get_task",
        &json!({"taskId":"missing"}),
    )
    .unwrap_err();
    assert!(matches!(task_error, McpHostServiceError::NotFound));
    let templates = execute_tool(context(), "forensics.list_report_templates", &json!({})).unwrap();
    assert_eq!(templates.as_array().unwrap().len(), 2);
    let history = execute_tool(context(), "forensics.get_report_history", &json!({})).unwrap();
    assert!(history.as_array().unwrap().is_empty());
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
    let correlation = execute_analysis_tool(
        analysis_context(),
        "forensics.get_correlation_snapshot",
        &json!({"offset":0,"limit":25}),
    )
    .unwrap();
    assert_eq!(correlation["offset"], 0);
    assert_eq!(correlation["limit"], 25);
    assert_eq!(correlation["hasMore"], false);
    assert!(matches!(
        execute_analysis_tool(
            analysis_context(),
            "forensics.get_correlation_snapshot",
            &json!({"limit":101})
        ),
        Err(McpHostServiceError::Invalid(_))
    ));
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
        "forensics.list_linux_services",
        "forensics.list_docker_containers",
        "forensics.list_docker_images",
        "forensics.get_android_device_info",
        "forensics.get_android_package_summary",
        "forensics.get_analysis_report",
    ];
    for name in source_tools {
        let result =
            execute_analysis_tool(context(), name, &json!({"dataSourceId":"outside-case"}));
        let error = result.unwrap_err();
        assert!(error.to_string().contains("outside-case"), "{name}");
    }
    let invalid = execute_analysis_tool(
        context(),
        "forensics.get_analysis_system_info",
        &json!({"dataSourceId":"outside-case","hostPath":"C:\\private"}),
    );
    assert!(matches!(invalid, Err(McpHostServiceError::Invalid(_))));
}
