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
