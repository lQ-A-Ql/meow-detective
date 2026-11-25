use app_services::analysis_service::{
    get_browser_history_summary, get_email_extraction_summary, get_evtx_event_summary,
    get_registry_extraction_summary,
};
use persistence_source::repositories::artifact_repo::ArtifactRepo;
use serde_json::json;

#[test]
#[ignore = "requires explicit FORENSICS_PUBLISHED_SOURCE_DB and FORENSICS_QUERY_REPORT"]
fn published_windows_source_summaries() {
    let path = std::path::PathBuf::from(
        std::env::var_os("FORENSICS_PUBLISHED_SOURCE_DB")
            .expect("set FORENSICS_PUBLISHED_SOURCE_DB to a retained source snapshot"),
    );
    let connection = persistence_source::control::opening::open_source_database_read_only(&path)
        .expect("open retained source snapshot");
    let generation: String = connection
        .query_row(
            "SELECT value FROM source_meta WHERE key='fjall_root'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let artifacts = ArtifactRepo::new(&connection);
    let count = artifacts.count().unwrap();
    let families = artifacts.count_by_family().unwrap();
    let registry = get_registry_extraction_summary(&connection, 0, 10).unwrap();
    let browser = get_browser_history_summary(&connection, 0, 10).unwrap();
    let email = get_email_extraction_summary(&connection, 0, 10).unwrap();
    let events = get_evtx_event_summary(&connection, None, 0, 10).unwrap();
    let report = json!({
        "sourceDb": path,
        "generation": generation,
        "artifactCount": count,
        "families": families,
        "registry": {"total": registry.total, "rows": registry.values.len()},
        "browser": {
            "visitTotal": browser.visit_total, "visits": browser.visits.len(),
            "cookieTotal": browser.cookie_total, "cookies": browser.cookies.len(),
            "downloadTotal": browser.download_total, "downloads": browser.downloads.len()
        },
        "email": {"total": email.total, "messages": email.messages.len(), "accounts": email.accounts.len()},
        "events": {
            "total": events.total_count, "pageTotal": events.page_total,
            "boot": events.boot_events.len(), "security": events.security_events.len(),
            "application": events.application_events.len()
        }
    });
    let output = std::env::var_os("FORENSICS_QUERY_REPORT").expect("set FORENSICS_QUERY_REPORT");
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    assert!(
        count > 0,
        "retained Windows source must have published artifacts"
    );
    assert_eq!(count, families.iter().map(|(_, count)| count).sum::<u64>());
    assert!(registry.total > 0, "published registry summary is empty");
    assert!(
        browser.visit_total > 0,
        "published browser summary is empty"
    );
}
