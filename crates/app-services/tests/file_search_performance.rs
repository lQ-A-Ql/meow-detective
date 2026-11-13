#[path = "support/file_search_case.rs"]
mod fixture;
#[path = "support/fjall_projection.rs"]
mod fjall_projection;

use app_services::search_service::{
    clear_source_file_search_cache, search_files_for_case, search_files_for_case_cursor,
    SearchError,
};
use fixture::{document, SearchCase};
use persistence_source::fjall::instrumentation::{ProcessMeasurement, StorageCounters};
use transport::commands::{
    FileSortDirectionDto, SearchEntryTypeDto, SearchFilesRequest, SearchSortKeyDto,
};
use transport::dto::SearchFileResultPageDto;

fn request(limit: u32) -> SearchFilesRequest {
    SearchFilesRequest {
        query: String::new(),
        match_path: false,
        entry_type: SearchEntryTypeDto::Any,
        extensions: Vec::new(),
        data_source_ids: Vec::new(),
        sort_key: SearchSortKeyDto::Name,
        sort_direction: FileSortDirectionDto::Asc,
        offset: 0,
        limit,
        cursor: None,
    }
}

fn page(
    case: &SearchCase,
    request: &SearchFilesRequest,
) -> Result<SearchFileResultPageDto, SearchError> {
    let connection = case.active.connection().unwrap();
    search_files_for_case_cursor(
        &connection,
        &case.active.case_root,
        &case.active.meta.id,
        request,
    )
}

fn measured_page(
    case: &SearchCase,
    request: &SearchFilesRequest,
) -> (SearchFileResultPageDto, StorageCounters) {
    let measurement = ProcessMeasurement::start().unwrap();
    let result = page(case, request).unwrap();
    let counters = measurement.snapshot();
    (result, counters)
}

#[test]
fn real_source_indexes_reuse_sessions_merge_pages_and_expire_generations() {
    let case = SearchCase::new();
    let documents = (0..5).map(document).collect::<Vec<_>>();
    for source in 0..64 {
        case.add_source(&format!("s{source:02}"), &documents);
    }
    let mut current = request(50);
    let (first, cold) = measured_page(&case, &current);
    assert_eq!(cold.search_index_opens, 64);
    assert_eq!(cold.query_session_opens, 64);
    assert!(cold.ranked_hits <= 64 * 50);
    assert!(cold.merge_comparisons > 0 && cold.merge_comparisons < 2_000);
    assert!(first.coverage.complete);
    let original_cursor = first.next_cursor.clone().unwrap();
    let mut actual = first
        .items
        .into_iter()
        .map(|item| item.file_id)
        .collect::<Vec<_>>();
    current.cursor = Some(original_cursor.clone());
    let (second, hot) = measured_page(&case, &current);
    assert_eq!(hot.search_index_opens, 0);
    assert_eq!(hot.query_session_opens, 0);
    assert!(hot.ranked_hits <= 64 * 50);
    actual.extend(second.items.into_iter().map(|item| item.file_id));
    current.cursor = second.next_cursor;
    while current.cursor.is_some() {
        let result = page(&case, &current).unwrap();
        actual.extend(result.items.into_iter().map(|item| item.file_id));
        current.cursor = result.next_cursor;
    }
    let expected = (0..5)
        .flat_map(|file| (0..64).map(move |source| format!("ds:s{source:02}:f{file}")))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    let mut offset = request(31);
    offset.offset = 137;
    let connection = case.active.connection().unwrap();
    let result = search_files_for_case(
        &connection,
        &case.active.case_root,
        &case.active.meta.id,
        &offset,
    )
    .unwrap();
    drop(connection);
    assert_eq!(
        result
            .items
            .iter()
            .map(|item| item.file_id.clone())
            .collect::<Vec<_>>(),
        expected[137..168]
    );

    clear_source_file_search_cache(&case.active.case_root, "s00").unwrap();
    let mut single = request(1);
    single.data_source_ids = vec!["s00".to_owned()];
    let (before, reopened) = measured_page(&case, &single);
    assert_eq!(reopened.search_index_opens, 1);
    assert_eq!(reopened.query_session_opens, 1);
    single.cursor = before.next_cursor;
    case.add_documents("s00", &[document(5)]);
    let refresh_measurement = ProcessMeasurement::start().unwrap();
    let stale_result = page(&case, &single);
    let refresh = refresh_measurement.snapshot();
    drop(refresh_measurement);
    assert!(matches!(stale_result, Err(SearchError::InvalidInput(_))));
    assert_eq!(refresh.search_index_opens, 0);
    assert_eq!(refresh.query_session_opens, 1);
    single.cursor = None;
    let (updated, changed) = measured_page(&case, &single);
    assert_eq!(updated.total, 6);
    assert_eq!(changed.search_index_opens, 0);
    assert_eq!(changed.query_session_opens, 0);
    single.cursor = updated.next_cursor;
    clear_source_file_search_cache(&case.active.case_root, "s00").unwrap();
    std::fs::remove_dir_all(case.index_path("s00")).unwrap();
    case.add_documents("s00", &(0..6).map(document).collect::<Vec<_>>());
    assert!(matches!(
        page(&case, &single),
        Err(SearchError::InvalidInput(_))
    ));

    case.add_source("s64", &documents);
    let (many, _) = measured_page(&case, &request(1));
    assert_eq!(many.coverage.ready_source_count, 65);
    let mut touch = request(1);
    for source in 0..65 {
        touch.data_source_ids = vec![format!("s{source:02}")];
        let touched = page(&case, &touch).unwrap();
        assert_eq!(touched.coverage.ready_source_count, 1);
        assert_eq!(touched.total, if source == 0 { 6 } else { 5 });
    }
    single.cursor = None;
    let (_, evicted) = measured_page(&case, &single);
    assert_eq!(evicted.search_index_opens, 1);
    assert_eq!(evicted.query_session_opens, 1);
    let mut oversized = single.clone();
    oversized.query = "a".repeat(8 * 1024 * 1024);
    assert!(matches!(
        page(&case, &oversized),
        Err(SearchError::Resources(_))
    ));
    let connection = case.active.connection().unwrap();
    app_services::case_service::delete_data_source_in(&connection, &case.active.case_root, "s64")
        .unwrap();
    drop(connection);
    let mut stale = request(1);
    stale.cursor = many.next_cursor;
    assert!(matches!(
        page(&case, &stale),
        Err(SearchError::InvalidInput(_))
    ));
}
