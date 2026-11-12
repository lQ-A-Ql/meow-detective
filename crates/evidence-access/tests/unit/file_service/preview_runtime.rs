use std::{thread, time::Duration};

use domain::{CaseId, DataSourceId};

use super::{PreviewRuntimeRegistry, PreviewSession};

fn routed_session(case_id: &str, source_id: &str, file_id: &str) -> PreviewSession {
    PreviewSession::routed(
        case_id.to_string(),
        source_id.to_string(),
        file_id.to_string(),
        4096,
        Some("application/octet-stream".to_string()),
    )
}

fn insert_routed_session(
    registry: &PreviewRuntimeRegistry,
    case_id: &str,
    source_id: &str,
    file_id: &str,
) -> String {
    let token = registry
        .begin_session(
            &CaseId(case_id.to_string()),
            &DataSourceId(source_id.to_string()),
        )
        .unwrap();
    registry
        .insert_session(&token, routed_session(case_id, source_id, file_id))
        .unwrap()
}

#[test]
fn opaque_handle_is_case_scoped_and_not_reversible() {
    let registry = PreviewRuntimeRegistry::default();
    let handle = insert_routed_session(&registry, "case-a", "source-a", "ds:source-a:file-a");

    assert!(handle.starts_with("preview:"));
    assert!(!handle.contains("case-a"));
    assert!(!handle.contains("source-a"));
    assert!(!handle.contains("file-a"));
    assert_eq!(
        registry
            .get_session("case-a", &handle)
            .unwrap()
            .global_file_id(),
        "ds:source-a:file-a"
    );
    assert!(registry.get_session("case-b", &handle).is_err());
}

#[test]
fn close_and_case_invalidation_expire_handles() {
    let registry = PreviewRuntimeRegistry::default();
    let first = insert_routed_session(&registry, "case-a", "source-a", "file-a");
    let second = insert_routed_session(&registry, "case-a", "source-b", "file-b");

    assert!(registry.close_session("case-a", &first).unwrap());
    assert!(registry.get_session("case-a", &first).is_err());
    registry.invalidate_case("case-a").unwrap();
    assert!(registry.get_session("case-a", &second).is_err());
}

#[test]
fn source_invalidation_does_not_evict_other_sources() {
    let registry = PreviewRuntimeRegistry::default();
    let first = insert_routed_session(&registry, "case-a", "source-a", "file-a");
    let second = insert_routed_session(&registry, "case-a", "source-b", "file-b");

    registry.invalidate_source("case-a", "source-a").unwrap();
    assert!(registry.get_session("case-a", &first).is_err());
    assert!(registry.get_session("case-a", &second).is_ok());
}

#[test]
fn session_budget_evicts_least_recently_used_handle() {
    let registry = PreviewRuntimeRegistry::new(Duration::from_secs(60), 2, 1);
    let first = insert_routed_session(&registry, "case-a", "source-a", "file-a");
    let second = insert_routed_session(&registry, "case-a", "source-a", "file-b");
    registry.get_session("case-a", &first).unwrap();
    let third = insert_routed_session(&registry, "case-a", "source-a", "file-c");

    assert!(registry.get_session("case-a", &first).is_ok());
    assert!(registry.get_session("case-a", &second).is_err());
    assert!(registry.get_session("case-a", &third).is_ok());
}

#[test]
fn expired_session_is_not_rebuilt_from_file_id() {
    let registry = PreviewRuntimeRegistry::new(Duration::from_millis(1), 2, 1);
    let handle = insert_routed_session(&registry, "case-a", "source-a", "file-a");
    thread::sleep(Duration::from_millis(5));

    assert!(registry.get_session("case-a", &handle).is_err());
}

#[test]
fn registry_stats_report_live_sessions_and_limits() {
    let registry = PreviewRuntimeRegistry::new(Duration::from_secs(60), 3, 2);
    insert_routed_session(&registry, "case-a", "source-a", "file-a");
    insert_routed_session(&registry, "case-a", "source-a", "file-b");

    let stats = registry.stats().unwrap();
    assert_eq!(stats.runtime_count, 0);
    assert_eq!(stats.filesystem_count, 0);
    assert_eq!(stats.session_count, 2);
    assert_eq!(stats.provider_constructions, 0);
    assert_eq!(stats.filesystem_constructions, 0);
    assert_eq!(stats.runtime_cache_capacity_bytes, 0);
    assert_eq!(stats.max_sessions, 3);
    assert_eq!(stats.max_runtimes, 2);
    assert_eq!(stats.max_filesystems, 16);
}

#[test]
fn invalidation_rejects_a_session_built_with_an_old_scope_token() {
    let registry = PreviewRuntimeRegistry::default();
    let case_id = CaseId("case-a".to_string());
    let source_id = DataSourceId("source-a".to_string());
    let token = registry.begin_session(&case_id, &source_id).unwrap();

    registry.invalidate_source("case-a", "source-a").unwrap();

    let error = registry
        .insert_session(&token, routed_session("case-a", "source-a", "file-a"))
        .expect_err("stale session must not be inserted");
    assert!(error.to_string().contains("no longer available"));
}

#[test]
fn retired_scope_waits_for_in_flight_reads_and_blocks_new_sessions() {
    let registry = std::sync::Arc::new(PreviewRuntimeRegistry::default());
    let handle = insert_routed_session(&registry, "case-a", "source-a", "file-a");
    let lease = registry.get_session("case-a", &handle).unwrap();
    let retiring_registry = registry.clone();
    let retire = thread::spawn(move || {
        retiring_registry
            .retire_source_and_drain("case-a", "source-a", Duration::from_secs(1))
            .unwrap()
    });

    thread::sleep(Duration::from_millis(20));
    assert!(!retire.is_finished());
    drop(lease);
    assert!(retire.join().unwrap());
    assert!(registry
        .begin_session(
            &CaseId("case-a".to_string()),
            &DataSourceId("source-a".to_string()),
        )
        .is_err());
}

#[test]
fn retired_scope_waits_for_an_in_flight_session_open() {
    let registry = std::sync::Arc::new(PreviewRuntimeRegistry::default());
    let token = registry
        .begin_session(
            &CaseId("case-a".to_string()),
            &DataSourceId("source-a".to_string()),
        )
        .unwrap();
    let retiring_registry = registry.clone();
    let retire = thread::spawn(move || {
        retiring_registry
            .retire_source_and_drain("case-a", "source-a", Duration::from_secs(1))
            .unwrap()
    });

    thread::sleep(Duration::from_millis(20));
    assert!(!retire.is_finished());
    drop(token);
    assert!(retire.join().unwrap());
}
