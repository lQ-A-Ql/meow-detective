use std::path::PathBuf;

use app_services::{
    analysis_service::{self, AnalysisSourceReadRuntime},
    mcp_host_service::{execute_analysis_tool, McpHostAnalysisQueryContext, McpHostQueryContext},
};
use domain::{CaseMeta, DataSourceId};
use serde_json::json;
use std::time::Instant;

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

#[test]
#[ignore = "requires an explicitly selected imported Linux case and data source"]
fn linux_summary_materialization_serves_the_cached_first_page() {
    let root = PathBuf::from(std::env::var_os("FORENSICS_MCP_CASE_ROOT").expect("case root"));
    let source = DataSourceId(std::env::var("FORENSICS_MCP_SOURCE_ID").expect("source ID"));
    let meta: CaseMeta =
        serde_json::from_slice(&std::fs::read(root.join("case.json")).unwrap()).unwrap();
    let conn = rusqlite::Connection::open_with_flags(
        root.join("app.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();

    let materialize_started = Instant::now();
    let materialized =
        analysis_service::materialize_source_linux_summary(&conn, &root, &meta.id, &source)
            .unwrap();
    let materialize_elapsed = materialize_started.elapsed();

    let cached_started = Instant::now();
    let cached =
        analysis_service::get_source_linux_summary(&conn, &root, &meta.id, &source, 0, 200)
            .unwrap();
    let cached_elapsed = cached_started.elapsed();

    eprintln!(
        "LINUX_SUMMARY_PERF materialize_ms={} cached_ms={} total_artifacts={} candidates_loaded={}",
        materialize_elapsed.as_millis(),
        cached_elapsed.as_millis(),
        cached.total_count,
        cached.journal_entries.len()
            + cached.login_records.len()
            + cached.bash_commands.len()
            + cached.apt_events.len()
            + cached.cron_jobs.len()
            + cached.sudo_events.len()
            + cached.system_configs.len()
            + cached.web_sites.len()
            + cached.web_access_logs.len()
            + cached.web_error_logs.len()
            + cached.web_findings.len()
            + cached.mysql_configs.len()
            + cached.mysql_logs.len()
            + cached.mysql_findings.len()
    );
    assert_eq!(cached.generated_at, materialized.generated_at);
    assert_eq!(cached.total_count, materialized.total_count);
    assert!(cached_elapsed < std::time::Duration::from_secs(5));
}

#[test]
#[ignore = "requires a materialized Linux case and explicit source ID"]
fn linux_summary_reads_existing_page_cache_in_a_cold_process() {
    let root = PathBuf::from(std::env::var_os("FORENSICS_MCP_CASE_ROOT").expect("case root"));
    let source = DataSourceId(std::env::var("FORENSICS_MCP_SOURCE_ID").expect("source ID"));
    let meta: CaseMeta =
        serde_json::from_slice(&std::fs::read(root.join("case.json")).unwrap()).unwrap();
    let conn = rusqlite::Connection::open_with_flags(
        root.join("app.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();

    let started = Instant::now();
    let summary =
        analysis_service::get_source_linux_summary(&conn, &root, &meta.id, &source, 0, 200)
            .unwrap();
    let elapsed = started.elapsed();
    eprintln!(
        "LINUX_SUMMARY_COLD_CACHE ms={} total_artifacts={}",
        elapsed.as_millis(),
        summary.total_count
    );
    assert!(summary.total_count > 0);
    assert!(elapsed < std::time::Duration::from_secs(5));
}
