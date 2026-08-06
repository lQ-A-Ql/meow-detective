use super::*;
use app_services::case_service;
use uuid::Uuid;

#[test]
fn active_case_snapshot_and_pool_connection_stay_in_sync() {
    let root = std::env::temp_dir().join(format!(
        "Meow_Detective-command-support-test-{}",
        Uuid::new_v4()
    ));
    let active = case_service::create_case(&root, "Command Support", Some("Codex Test")).unwrap();
    let state = AppState::default();

    assert!(snapshot_active_case(&state).unwrap().is_none());

    *state.active_case.lock().unwrap() = Some(active);
    state.init_db_pragmas().unwrap();

    let snapshot = require_active_case(&state).unwrap();
    assert_eq!(snapshot.case_root.parent(), Some(root.as_path()));
    get_case_connection(&state).unwrap();

    *state.active_case.lock().unwrap() = None;
    let err = require_active_case(&state).unwrap_err();
    assert_eq!(err.code, "NO_ACTIVE_CASE");

    state.clear_db_state().unwrap();
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn emulation_audit_events_map_to_their_persisted_actions() {
    assert_eq!(
        EmulationAuditEvent::Prepare.action().as_str(),
        "emulation.prepare"
    );
    assert_eq!(
        EmulationAuditEvent::Launch.action().as_str(),
        "emulation.launch"
    );
    assert_eq!(
        EmulationAuditEvent::Release.action().as_str(),
        "emulation.release"
    );
}
