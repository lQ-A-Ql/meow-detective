use app_services::storage_recovery_service::{repair_operation, scan_case, RecoveryDisposition};
use chrono::Utc;
use persistence_sqlite::repositories::storage_control_repo::{
    StorageControlRepo, StorageOperationRecord, StorageOperationState,
};

#[test]
fn startup_recovery_reports_repair_for_an_unbound_open_operation() {
    let root = tempfile::tempdir().unwrap();
    let case = app_services::case_service::create_case(root.path(), "Recovery", None).unwrap();
    case.with_conn(|connection| {
        StorageControlRepo::new(connection).insert_operation(&StorageOperationRecord {
            operation_id: "op-recovery".into(),
            case_id: case.meta.id.0.clone(),
            source_id: None,
            attempt_id: "attempt-1".into(),
            operation_kind: "source_import".into(),
            state: StorageOperationState::Running,
            input_digest: None,
            expected_batch_digest: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            last_error_code: None,
        })
    })
    .unwrap();
    let report = case
        .with_conn(|connection| {
            scan_case(connection, &case.case_root, &case.meta.id.0)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    assert_eq!(report.disposition, RecoveryDisposition::RepairRequired);
    assert_eq!(report.items.len(), 1);
}

#[test]
fn repair_operation_persists_failure_state_and_repair_event() {
    let root = tempfile::tempdir().unwrap();
    let case = app_services::case_service::create_case(root.path(), "Repair", None).unwrap();
    case.with_conn(|connection| {
        StorageControlRepo::new(connection).insert_operation(&StorageOperationRecord {
            operation_id: "op-repair".into(),
            case_id: case.meta.id.0.clone(),
            source_id: None,
            attempt_id: "attempt".into(),
            operation_kind: "source_import".into(),
            state: StorageOperationState::Running,
            input_digest: None,
            expected_batch_digest: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            last_error_code: None,
        })?;
        repair_operation(connection, "op-repair", "manual-repair")
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        let state = StorageControlRepo::new(connection)
            .find_operation("op-repair")?
            .unwrap()
            .state;
        assert_eq!(state, StorageOperationState::Failed);
        Ok(())
    })
    .unwrap();
}
