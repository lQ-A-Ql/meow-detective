use super::*;

#[test]
fn report_history_item_serializes_existing_contract_only_as_camel_case() {
    let dto = ReportHistoryItemDto {
        id: "report-1".to_string(),
        file_name: "case-summary.html".to_string(),
        created_by: "examiner".to_string(),
        created_at: "2026-06-04T00:00:00Z".to_string(),
        status: "running".to_string(),
        progress: Some(60),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["fileName"], "case-summary.html");
    assert_eq!(value["createdBy"], "examiner");
    assert_eq!(value["createdAt"], "2026-06-04T00:00:00Z");
    assert_eq!(value["progress"], 60);
    assert!(value.get("file_name").is_none());
    assert!(value.get("created_by").is_none());
    assert!(value.get("exportScope").is_none());
    assert!(value.get("provenance").is_none());
}

#[test]
fn report_history_item_skips_missing_progress() {
    let dto = ReportHistoryItemDto {
        id: "report-1".to_string(),
        file_name: "case-summary.html".to_string(),
        created_by: "examiner".to_string(),
        created_at: "2026-06-04T00:00:00Z".to_string(),
        status: "completed".to_string(),
        progress: None,
    };

    let value = serde_json::to_value(dto).unwrap();

    assert!(value.get("progress").is_none());
}
