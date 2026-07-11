use app_services::case_service;
use transport::dto::batch::{BatchPlanDto, BatchResourceLimitsDto};
use uuid::Uuid;

use super::lifecycle::{cancel_batch_impl, pause_batch_impl, resume_batch_impl, start_batch_impl};
use crate::commands::command_support::require_active_case;
use crate::state::AppState;

#[test]
fn batch_commands_require_active_case() {
    let state = AppState::default();
    let result = require_active_case(&state);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, "NO_ACTIVE_CASE");
}

#[test]
fn batch_plan_roundtrip_persists_job() {
    let root =
        std::env::temp_dir().join(format!("forensics-batch-command-test-{}", Uuid::new_v4()));
    let active = case_service::create_case(&root, "Batch Command", Some("Codex Test")).unwrap();
    let case_id = active.meta.id.0.clone();
    let state = AppState::default();
    *state.active_case.lock().unwrap() = Some(active);
    state.init_db_pragmas().unwrap();

    let connection = state.get_connection().unwrap();
    let job = app_services::batch_service::create_and_persist_batch(
        &connection,
        &case_id,
        "test-plan",
        BatchPlanDto {
            data_source_refs: vec!["ds-1".to_string()],
            phases: vec!["Mount".to_string(), "Catalog".to_string()],
            resource_limits: BatchResourceLimitsDto {
                max_memory_mb: Some(1024),
                max_threads: Some(2),
            },
        },
    )
    .unwrap();

    assert_eq!(job.label, "test-plan");
    assert_eq!(job.phases.len(), 2);
    let listed = app_services::batch_service::list_batch_jobs(&connection, &case_id).unwrap();
    assert_eq!(listed.len(), 1);

    state.clear_db_state().unwrap();
    std::fs::remove_dir_all(root).ok();
}

fn setup_active_case_with_batch_job() -> (std::path::PathBuf, AppState, String) {
    let root = std::env::temp_dir().join(format!("forensics-batch-stub-test-{}", Uuid::new_v4()));
    let active = case_service::create_case(&root, "Batch Stub", Some("Codex Test")).unwrap();
    let case_id = active.meta.id.0.clone();
    let state = AppState::default();
    *state.active_case.lock().unwrap() = Some(active);
    state.init_db_pragmas().unwrap();

    let connection = state.get_connection().unwrap();
    let job = app_services::batch_service::create_and_persist_batch(
        &connection,
        &case_id,
        "stub-plan",
        BatchPlanDto {
            data_source_refs: vec!["ds-1".to_string()],
            phases: vec!["Mount".to_string()],
            resource_limits: BatchResourceLimitsDto {
                max_memory_mb: None,
                max_threads: None,
            },
        },
    )
    .unwrap();
    (root, state, job.id)
}

fn assert_unsupported(
    run: impl std::future::Future<
        Output = Result<transport::dto::batch::BatchJobDto, transport::CommandError>,
    >,
) {
    let error = tauri::async_runtime::block_on(run).unwrap_err();
    assert_eq!(error.code, "UNSUPPORTED");
    assert!(error.message.to_ascii_lowercase().contains("not supported"));
}

#[test]
fn start_batch_returns_unsupported_stub() {
    let (root, state, batch_id) = setup_active_case_with_batch_job();
    assert_unsupported(start_batch_impl(&state, batch_id));
    state.clear_db_state().unwrap();
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn pause_batch_returns_unsupported_stub() {
    let (root, state, batch_id) = setup_active_case_with_batch_job();
    assert_unsupported(pause_batch_impl(&state, batch_id));
    state.clear_db_state().unwrap();
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn resume_batch_returns_unsupported_stub() {
    let (root, state, batch_id) = setup_active_case_with_batch_job();
    assert_unsupported(resume_batch_impl(&state, batch_id));
    state.clear_db_state().unwrap();
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn cancel_batch_returns_unsupported_stub() {
    let (root, state, batch_id) = setup_active_case_with_batch_job();
    assert_unsupported(cancel_batch_impl(&state, batch_id));
    state.clear_db_state().unwrap();
    std::fs::remove_dir_all(root).ok();
}
