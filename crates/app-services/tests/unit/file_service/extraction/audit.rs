use super::*;
use persistence_sqlite::repositories::audit_repo::AuditRepo;

#[test]
fn audit_record_preserves_case_resource_and_details() {
    let connection = persistence_sqlite::open_in_memory().expect("open audit database");
    persistence_sqlite::runner::run_all(&connection).expect("run audit schema migrations");

    record_file_extraction_audit(
        &connection,
        Some("case-extract"),
        "ds:source:file",
        &serde_json::json!({"status": "ok", "bytesWritten": 10}),
    )
    .expect("persist extraction audit");

    let entries = AuditRepo::new(&connection)
        .query(Some("case-extract"), Some("file.extract"), 10, 0)
        .expect("query extraction audit");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].resource_id.as_deref(), Some("ds:source:file"));
    assert!(entries[0].details.contains("bytesWritten"));
}

#[test]
fn audit_record_reports_missing_schema() {
    let connection = rusqlite::Connection::open_in_memory().expect("open unmigrated database");
    let error = record_file_extraction_audit(
        &connection,
        Some("case-extract"),
        "ds:source:file",
        &serde_json::json!({"status": "ok"}),
    )
    .expect_err("missing audit schema must be reported");

    assert!(matches!(error, FileServiceError::Db(_)));
}
