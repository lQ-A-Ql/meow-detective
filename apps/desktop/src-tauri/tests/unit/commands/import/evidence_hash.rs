use std::sync::Arc;
use std::time::Duration;

use app_services::{case_service, datasource_service};
use domain::{DataSourceHashStatus, DataSourceKind, DataSourcePlatform};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;

use super::*;
use crate::commands::import::background_job::evidence_hash::progress::hash_progress_percent;

#[test]
fn pending_ready_source_hashes_in_background_without_duplicate_job() {
    let temporary = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temporary.path(), "hash", Some("tester")).unwrap();
    let evidence = temporary.path().join("evidence.raw");
    std::fs::write(&evidence, b"background evidence hash").unwrap();
    let connection = app_services::connection::open_case_db(&active.db_path()).unwrap();
    let source = datasource_service::attach_data_source(
        &connection,
        &active.meta.id,
        "evidence.raw",
        &evidence,
        DataSourceKind::Raw,
        DataSourcePlatform::Windows,
    )
    .unwrap();
    DataSourceRepo::new(&connection)
        .update_import_state(&source.id, "ready", None)
        .unwrap();
    let manager = Arc::new(TaskManager::new());

    let jobs = schedule_pending_evidence_hashes(
        &active.case_root,
        &active.meta.id.0,
        None,
        Arc::clone(&manager),
    )
    .unwrap();
    assert_eq!(jobs.len(), 1);
    let result = manager
        .wait_task(&hash_task_id(&source.id), Duration::from_secs(5))
        .unwrap();
    assert!(result.is_ok());

    let stored = DataSourceRepo::new(&connection)
        .find_by_case(&active.meta.id)
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.id == source.id)
        .unwrap();
    assert_eq!(stored.provenance.hash_status, DataSourceHashStatus::Hashed);
    assert_eq!(
        stored.provenance.source_hash_sha256,
        Some(HashService::sha256_bytes(b"background evidence hash"))
    );
    let duplicate = schedule_pending_evidence_hashes(
        &active.case_root,
        &active.meta.id.0,
        None,
        Arc::clone(&manager),
    )
    .unwrap();
    assert!(duplicate.is_empty());
}

#[test]
fn cluster_hash_scheduler_keeps_one_large_source_in_flight_and_handoffs_next() {
    let temporary = tempfile::tempdir().unwrap();
    let active =
        case_service::create_case(temporary.path(), "hash-sequence", Some("tester")).unwrap();
    let first_path = temporary.path().join("first.raw");
    let second_path = temporary.path().join("second.raw");
    std::fs::write(&first_path, vec![b'a'; 256 * 1024]).unwrap();
    std::fs::write(&second_path, vec![b'b'; 256 * 1024]).unwrap();
    let connection = app_services::connection::open_case_db(&active.db_path()).unwrap();
    let first = datasource_service::attach_data_source(
        &connection,
        &active.meta.id,
        "first.raw",
        &first_path,
        DataSourceKind::Raw,
        DataSourcePlatform::Windows,
    )
    .unwrap();
    let second = datasource_service::attach_data_source(
        &connection,
        &active.meta.id,
        "second.raw",
        &second_path,
        DataSourceKind::Raw,
        DataSourcePlatform::Windows,
    )
    .unwrap();
    let repo = DataSourceRepo::new(&connection);
    repo.update_import_state(&first.id, "ready", None).unwrap();
    repo.update_import_state(&second.id, "ready", None).unwrap();
    drop(connection);
    let manager = Arc::new(TaskManager::new());
    let jobs = schedule_pending_evidence_hashes(
        &active.case_root,
        &active.meta.id.0,
        None,
        Arc::clone(&manager),
    )
    .unwrap();
    assert_eq!(
        jobs.len(),
        1,
        "hash scheduler must admit one disk-bound source at a time"
    );
    let active_source = if manager.is_running(&hash_task_id(&first.id)) {
        first.id.clone()
    } else {
        second.id.clone()
    };
    let waiting_source = if active_source == first.id {
        second.id.clone()
    } else {
        first.id.clone()
    };
    let first_result = manager
        .wait_task(&hash_task_id(&active_source), Duration::from_secs(10))
        .unwrap();
    assert!(first_result.is_ok());
    let second_result = manager
        .wait_task(&hash_task_id(&waiting_source), Duration::from_secs(10))
        .unwrap();
    assert!(
        second_result.is_ok(),
        "next pending source should be scheduled after the first settles"
    );
}

#[test]
fn progress_percent_reserves_terminal_completion() {
    assert_eq!(hash_progress_percent(0, 100), 2);
    assert_eq!(hash_progress_percent(50, 100), 50);
    assert_eq!(hash_progress_percent(100, 100), 98);
    assert_eq!(hash_progress_percent(1, 0), 98);
}
