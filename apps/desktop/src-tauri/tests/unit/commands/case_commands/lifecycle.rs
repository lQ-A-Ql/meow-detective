use crate::commands::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;
use app_services::case_service;
use uuid::Uuid;

#[test]
fn active_case_pool_is_guarded_by_active_case_lifecycle() {
    let root = std::env::temp_dir().join(format!(
        "Meow_Detective-pool-lifecycle-test-{}",
        Uuid::new_v4()
    ));
    let active = case_service::create_case(&root, "Pool Lifecycle", Some("Codex Test"))
        .expect("create test case");
    let state = AppState::default();

    let no_active_case = state
        .get_connection()
        .expect_err("pool access must require active case");
    assert!(no_active_case.contains("No active case"));

    *state.active_case.lock().expect("active case lock") = Some(active);
    state.init_db_pragmas().expect("initialize pragmas");
    state
        .get_connection()
        .expect("pool available when active case is set");

    state.clear_db_state().expect("clear pool");
    *state.active_case.lock().expect("active case lock") = None;
    let cleared = state
        .get_connection()
        .expect_err("cleared pool must not be usable");
    assert!(cleared.contains("No active case"));

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn command_support_helpers_follow_active_case_lifecycle() {
    let root = std::env::temp_dir().join(format!(
        "Meow_Detective-command-helper-lifecycle-test-{}",
        Uuid::new_v4()
    ));
    let active = case_service::create_case(&root, "Lifecycle", Some("Codex Test"))
        .expect("create test case");
    let case_root = active.case_root.clone();
    let state = AppState::default();

    let no_case = require_active_case(&state).expect_err("active case required");
    assert_eq!(no_case.code, "NO_ACTIVE_CASE");

    *state.active_case.lock().expect("active case lock") = Some(active);
    state.init_db_pragmas().expect("initialize pragmas");
    let snapshot = require_active_case(&state).expect("snapshot available");
    assert_eq!(snapshot.case_root, case_root);
    get_case_connection(&state).expect("connection available");

    *state.active_case.lock().expect("active case lock") = None;
    let no_case_again = get_case_connection(&state).expect_err("connection requires case");
    assert_eq!(no_case_again.code, "NO_ACTIVE_CASE");

    state.clear_db_state().expect("clear pool");
    std::fs::remove_dir_all(root).ok();
}
