use super::*;
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::audit_repo::AuditRepo;

fn case_connection() -> Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open case database");
    persistence_sqlite::runner::run_all(&connection).expect("run case migrations");
    connection
}

#[test]
fn plugin_audit_trail_records_load_reject_and_extract_failure() {
    let connection = case_connection();
    let loads = vec![PluginLoadRecord {
        plugin_id: "meow.fixture.good".to_string(),
        plugin_version: "0.1.0".to_string(),
    }];
    let rejections = vec![PluginRejection {
        path: std::path::PathBuf::from("plugins/bad.dll"),
        reason: "ABI version 1 != host 2".to_string(),
    }];
    let failures = vec![PluginExtractFailure {
        plugin_id: "meow.fixture.good".to_string(),
        source_path: "[P0]/Evidence/FOO.MFX".to_string(),
        error: "payload is not valid JSON".to_string(),
    }];

    record_plugin_audit_trail(
        &connection,
        &CaseId("case-1".to_string()),
        &DataSourceId("source-1".to_string()),
        &loads,
        &rejections,
        &failures,
    );

    let repo = AuditRepo::new(&connection);
    let load_entries = repo
        .query(Some("case-1"), Some("plugin.load"), 10, 0)
        .expect("query plugin.load");
    assert_eq!(load_entries.len(), 1);
    assert_eq!(
        load_entries[0].resource_id.as_deref(),
        Some("meow.fixture.good")
    );
    assert_eq!(load_entries[0].resource_type, "plugin");
    assert!(load_entries[0].details.contains("0.1.0"));

    let reject_entries = repo
        .query(Some("case-1"), Some("plugin.reject"), 10, 0)
        .expect("query plugin.reject");
    assert_eq!(reject_entries.len(), 1);
    assert!(reject_entries[0].details.contains("ABI version"));

    let failure_entries = repo
        .query(Some("case-1"), Some("plugin.extract_failed"), 10, 0)
        .expect("query plugin.extract_failed");
    assert_eq!(failure_entries.len(), 1);
    assert_eq!(
        failure_entries[0].resource_id.as_deref(),
        Some("meow.fixture.good")
    );
    assert!(failure_entries[0].details.contains("[P0]/Evidence/FOO.MFX"));
}

#[test]
fn empty_plugin_audit_trail_writes_nothing() {
    let connection = case_connection();
    record_plugin_audit_trail(
        &connection,
        &CaseId("case-1".to_string()),
        &DataSourceId("source-1".to_string()),
        &[],
        &[],
        &[],
    );
    let count = AuditRepo::new(&connection)
        .count(Some("case-1"))
        .expect("count audit entries");
    assert_eq!(count, 0);
}
