use super::*;

fn setup() -> Connection {
    let connection = crate::connection::open_in_memory().unwrap();
    crate::runner::run_all(&connection).unwrap();
    connection.execute("INSERT INTO cases (id, name, created_at, updated_at) VALUES ('case-1', 'Report case', datetime('now'), datetime('now'))", []).unwrap();
    connection
}

fn record() -> ReportRecord {
    ReportRecord {
        id: "report-1".into(), case_id: "case-1".into(), template_id: "structured-summary".into(), file_name: "report.html".into(),
        created_by: "examiner".into(), status: "running".into(), progress: Some(0), created_at: "2026-11-26T00:00:00Z".into(),
        request_json: serde_json::json!({"caseId":"case-1","format":"html","scope":{"operatingSystem":true,"hardware":true,"browsers":true,"email":true}}).to_string(),
        error_code: None, error_message: None, file_size: None, sha256: None,
    }
}

#[test]
fn completed_report_preserves_size_hash_and_case_scope() {
    let connection = setup();
    let repo = ReportRepo::new(&connection);
    repo.insert(&record()).unwrap();
    repo.update_status("report-1", "running", Some(50)).unwrap();
    repo.complete("report-1", 123, &"a".repeat(64)).unwrap();
    let saved = repo.find_by_id("case-1", "report-1").unwrap().unwrap();
    assert_eq!(saved.status, "completed");
    assert_eq!(saved.progress, Some(100));
    assert_eq!(saved.file_size, Some(123));
    assert_eq!(saved.sha256.as_deref(), Some("a".repeat(64).as_str()));
    assert!(repo.find_by_id("other-case", "report-1").unwrap().is_none());
}

#[test]
fn failed_report_retains_failure_progress_and_diagnostic() {
    let connection = setup();
    let repo = ReportRepo::new(&connection);
    repo.insert(&record()).unwrap();
    repo.update_status("report-1", "running", Some(50)).unwrap();
    repo.fail("report-1", "failed", "REPORT_IO", "报告文件写入失败")
        .unwrap();
    let records = repo.list_by_case("case-1").unwrap();
    assert_eq!(records[0].status, "failed");
    assert_eq!(records[0].progress, Some(50));
    assert_eq!(records[0].error_code.as_deref(), Some("REPORT_IO"));
    assert!(records[0].file_size.is_none());
    assert!(repo.list_by_case("other-case").unwrap().is_empty());
}
