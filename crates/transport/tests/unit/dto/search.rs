use super::*;

#[test]
fn search_page_round_trip_preserves_result_window_metadata() {
    let dto = SearchResultPageDto {
        total: 120_000,
        available: 100_000,
        truncated: true,
        took_ms: 17,
        items: vec![SearchHitDto {
            file_id: "ds:source-1:file-1".to_string(),
            path: "/evidence/file-1.txt".to_string(),
            score: 1.25,
            snippets: vec![SearchSnippetDto {
                text: "matching text".to_string(),
                highlights: vec![SearchHighlightDto { start: 0, end: 8 }],
            }],
        }],
        next_cursor: Some("v1.payload.digest".to_string()),
    };

    let value = serde_json::to_value(&dto).unwrap();
    assert_eq!(value["available"], 100_000);
    assert_eq!(value["truncated"], true);
    assert_eq!(value["tookMs"], 17);
    assert_eq!(value["items"][0]["fileId"], "ds:source-1:file-1");
    assert_eq!(value["nextCursor"], "v1.payload.digest");

    let decoded: SearchResultPageDto = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.total, 120_000);
    assert_eq!(decoded.available, 100_000);
    assert!(decoded.truncated);
    assert_eq!(decoded.items.len(), 1);
    assert_eq!(decoded.next_cursor.as_deref(), Some("v1.payload.digest"));
}

#[test]
fn file_search_page_round_trip_preserves_metadata_and_coverage() {
    let dto = SearchFileResultPageDto {
        total: 1,
        available: 1,
        truncated: false,
        took_ms: 3,
        items: vec![SearchFileHitDto {
            file_id: "ds:source-1:file-1".to_string(),
            data_source_id: "source-1".to_string(),
            data_source_name: "Evidence".to_string(),
            name: "report.txt".to_string(),
            path: "/Downloads/report.txt".to_string(),
            entry_type: "file".to_string(),
            extension: Some("txt".to_string()),
            size: Some(42),
            modified_at: Some("2026-07-30T00:00:00Z".to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: true,
        }],
        coverage: SearchCoverageDto {
            ready_source_count: 2,
            indexed_source_count: 1,
            expected_entry_count: 10,
            indexed_entry_count: 8,
            missing_source_ids: vec!["source-2".to_string()],
            complete: false,
        },
        next_cursor: Some("v1.payload.digest".to_string()),
    };

    let value = serde_json::to_value(&dto).unwrap();
    assert_eq!(value["items"][0]["dataSourceId"], "source-1");
    assert_eq!(value["items"][0]["modifiedAt"], "2026-07-30T00:00:00Z");
    assert_eq!(value["coverage"]["expectedEntryCount"], 10);
    assert_eq!(value["coverage"]["missingSourceIds"][0], "source-2");

    let decoded: SearchFileResultPageDto = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.items[0].name, "report.txt");
    assert!(decoded.items[0].encrypted);
    assert!(!decoded.coverage.complete);
    assert_eq!(decoded.next_cursor.as_deref(), Some("v1.payload.digest"));
}
