use std::path::PathBuf;

use app_services::{
    analysis_service::{self, AnalysisSourceReadRuntime},
    mcp_host_service::{execute_analysis_tool, McpHostAnalysisQueryContext, McpHostQueryContext},
};
use domain::{CaseMeta, DataSourceId};
use serde_json::json;

#[test]
#[ignore = "requires an explicitly selected imported Linux case and data source"]
fn linux_config_tools_match_the_analysis_service() {
    let root = PathBuf::from(std::env::var_os("FORENSICS_MCP_CASE_ROOT").expect("case root"));
    let source = DataSourceId(std::env::var("FORENSICS_MCP_SOURCE_ID").expect("source ID"));
    let meta: CaseMeta =
        serde_json::from_slice(&std::fs::read(root.join("case.json")).unwrap()).unwrap();
    let conn = rusqlite::Connection::open_with_flags(
        root.join("app.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let runtime = AnalysisSourceReadRuntime::default();
    for (tool, kind) in [
        ("forensics.list_linux_services", "systemdServiceUnit"),
        ("forensics.list_docker_containers", "dockerContainer"),
        ("forensics.list_docker_images", "dockerImage"),
    ] {
        let expected = analysis_service::get_source_linux_system_config_page(
            &conn, &root, &meta.id, &source, kind, 0, 25,
        )
        .unwrap();
        let result = execute_analysis_tool(
            McpHostAnalysisQueryContext {
                query: McpHostQueryContext {
                    connection: &conn,
                    case_root: &root,
                    case_meta: &meta,
                },
                runtime: &runtime,
            },
            tool,
            &json!({"dataSourceId": source.0, "offset": 0, "limit": 25}),
        )
        .unwrap();
        assert_eq!(result, serde_json::to_value(expected).unwrap(), "{tool}");
    }
}
