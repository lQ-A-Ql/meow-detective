use super::{
    config::RunConfig,
    metrics::{measure, ScenarioCounters},
    native_fixture::NativeFixture,
    native_records::{seed_artifacts, seed_timeline, ARTIFACT_TYPE, TIMELINE_TYPE},
};
use transport::commands::{
    FileSortDirectionDto, SearchEntryTypeDto, SearchFilesRequest, SearchSortKeyDto,
};

pub(super) fn run(config: &RunConfig, scenario: &str) -> serde_json::Value {
    let fixture = NativeFixture::new(1, config.scale.core_files);
    if scenario == "artifact_list" {
        seed_artifacts(&fixture, 32, None);
    }
    if scenario == "timeline_filter" {
        seed_timeline(&fixture, 64);
    }
    let request = transport::commands::ExportReportRequest {
        case_id: fixture.active.meta.id.0.clone(),
        format: transport::dto::ReportFormatDto::Html,
        scope: transport::commands::ReportScopeDto::default(),
        data_source_id: None,
    };
    let workload =
        serde_json::json!({"files": config.scale.core_files, "sources": 1, "requests": 1});
    fixture.active.with_conn(|connection| {
        Ok(measure(config, scenario, workload, || {
            let rows = match scenario {
                "search_query" => {
                    let request = SearchFilesRequest {
                        query: "item".into(), match_path: false, entry_type: SearchEntryTypeDto::File,
                        extensions: Vec::new(), data_source_ids: Vec::new(),
                        sort_key: SearchSortKeyDto::Name, sort_direction: FileSortDirectionDto::Asc,
                        offset: 0, limit: 10, cursor: None,
                    };
                    let page = app_services::search_service::search_files_for_case_cursor(
                        connection, &fixture.active.case_root, &fixture.active.meta.id, &request,
                    ).unwrap();
                    assert_eq!(page.total, config.scale.core_files as u64);
                    assert!(page.items.iter().all(|item| item.name.starts_with("item-")));
                    page.items.len()
                }
                "file_tree_expand" => app_services::file_service::get_file_tree_for_case(
                    connection, &fixture.active.case_root, &fixture.active.meta.id, false,
                ).unwrap().len(),
                "file_paginate" => app_services::file_service::list_file_entries_page_for_case(
                    connection, &fixture.active.case_root, &fixture.active.meta.id,
                    &fixture.sources[0], None, 0, 10,
                ).unwrap().rows.len(),
                "timeline_filter" => {
                    let page = app_services::timeline_service::query_timeline_filtered_for_case(
                        connection, &fixture.active.case_root, &fixture.active.meta.id,
                        app_services::timeline_service::TimelineQuery {
                            offset: 0, limit: 20, time_start: None, time_end: None,
                            event_type: Some(TIMELINE_TYPE), cursor: None,
                        },
                    ).unwrap();
                    assert_eq!(page.total, 64);
                    page.items.len()
                }
                "artifact_list" => {
                    let page = app_services::artifact_service::get_artifact_rows_page_with_cursor_for_case(
                        connection, &fixture.active.case_root, &fixture.active.meta.id,
                        Some(ARTIFACT_TYPE), 0, 20, None,
                    ).unwrap();
                    assert_eq!(page.total, 32);
                    page.items.len()
                }
                "report_export" => {
                    let report = app_services::report::export_report_for_case(app_services::report::ReportExportContext { connection, case: &fixture.active.meta, case_root: &fixture.active.case_root, request: &request, cancel: &std::sync::atomic::AtomicBool::new(false) }).unwrap();
                    assert!(std::fs::metadata(fixture.active.case_root.join("reports").join(report.file_name)).unwrap().len() > 0);
                    1
                }
                _ => unreachable!(),
            };
            assert!(rows > 0, "benchmark {scenario} produced no results");
            ScenarioCounters { returned_rows: Some(rows as u64), ..Default::default() }
        }))
    }).unwrap()
}
