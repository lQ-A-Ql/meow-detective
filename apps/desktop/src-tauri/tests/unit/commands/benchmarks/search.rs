use super::{
    config::RunConfig,
    metrics::{measure, ScenarioCounters},
    native_fixture::NativeFixture,
};
use transport::commands::{
    FileSortDirectionDto, SearchEntryTypeDto, SearchFilesRequest, SearchSortKeyDto,
};

pub(super) fn run(config: &RunConfig, scenario: &str, offset: bool) -> serde_json::Value {
    let fixture = NativeFixture::new(
        config.scale.search_sources,
        config.scale.search_files_per_source,
    );
    fixture.active.with_conn(|connection| {
        Ok(measure(config, scenario,
            serde_json::json!({"sources": fixture.sources.len(), "filesPerSource": config.scale.search_files_per_source,
                "pageSize": 50, "pages": if offset {1} else {2}, "scanEnd": if offset {1000} else {50}}),
            || {
                let request = SearchFilesRequest {
                    query: String::new(), match_path: false, entry_type: SearchEntryTypeDto::File,
                    extensions: Vec::new(), data_source_ids: Vec::new(), sort_key: SearchSortKeyDto::Name,
                    sort_direction: FileSortDirectionDto::Asc, offset: if offset {950} else {0},
                    limit: 50, cursor: None,
                };
                let page = if offset {
                    app_services::search_service::search_files_for_case(
                        connection, &fixture.active.case_root, &fixture.active.meta.id, &request,
                    ).unwrap()
                } else {
                    app_services::search_service::search_files_for_case_cursor(
                        connection, &fixture.active.case_root, &fixture.active.meta.id, &request,
                    ).unwrap()
                };
                assert!(page.coverage.complete);
                assert_eq!(page.items.len(), 50);
                let mut returned = page.items.len() as u64;
                if !offset {
                    let mut continued = request;
                    continued.cursor = Some(page.next_cursor.expect("second search page"));
                    let next = app_services::search_service::search_files_for_case_cursor(
                        connection, &fixture.active.case_root, &fixture.active.meta.id, &continued,
                    ).unwrap();
                    assert_eq!(next.items.len(), 50);
                    assert!(page.items.iter().all(|first| next.items.iter().all(|second| first.file_id != second.file_id)));
                    returned += next.items.len() as u64;
                }
                ScenarioCounters { returned_rows: Some(returned), ..Default::default() }
            },
        ))
    }).unwrap()
}
