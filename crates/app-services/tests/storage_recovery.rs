use app_services::storage_recovery_service::{repair_operation, scan_case, RecoveryDisposition};
use chrono::Utc;
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
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
    let status = case
        .with_conn(|connection| {
            StorageControlRepo::new(connection).list_recovery_status(&case.meta.id.0)
        })
        .unwrap();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].disposition, "repair_required");
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

#[test]
fn startup_recovery_publishes_a_sealed_import_generation_and_finishes_its_operation() {
    let root = tempfile::tempdir().unwrap();
    let case =
        app_services::case_service::create_case(root.path(), "Generation Recovery", None).unwrap();
    let source_id = DataSourceId("generation-recovery-source".to_owned());
    let source = DataSource {
        id: source_id.clone(),
        name: "Generation recovery source".to_owned(),
        kind: DataSourceKind::E01,
        source_path: root.path().join("source.E01"),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source_id.0, Some("linux"), None);
    storage.import_state = "importing".to_owned();
    case.with_conn(|connection| {
        DataSourceRepo::new(connection).insert_with_storage(&case.meta.id, &source, &storage)?;
        StorageControlRepo::new(connection).insert_operation(&StorageOperationRecord {
            operation_id: "source-import:attempt-1".to_owned(),
            case_id: case.meta.id.0.clone(),
            source_id: Some(source_id.0.clone()),
            attempt_id: "attempt-1".to_owned(),
            operation_kind: "source_import".to_owned(),
            state: StorageOperationState::Running,
            input_digest: None,
            expected_batch_digest: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            last_error_code: None,
        })
    })
    .unwrap();

    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        &case.case_root,
        &source_id.0,
        "attempt-1",
    )
    .unwrap();
    std::fs::create_dir_all(layout.build_path()).unwrap();
    let build = persistence_source::fjall::SourceStore::open(
        layout.build_path(),
        persistence_source::fjall::DatabaseConfig::default(),
    )
    .unwrap();
    let projection_receipt = build
        .database()
        .write_batch(
            &[persistence_source::fjall::WriteAction::Put {
                keyspace: persistence_source::fjall::KeyspaceName::Meta,
                key: b"projection-complete".to_vec(),
                value: b"source-import".to_vec(),
            }],
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .unwrap();
    persistence_source::fjall::seal_source_generation_with_control(
        &build,
        &source_id.0,
        "attempt-1",
        &hex::encode(projection_receipt.batch_digest),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    drop(build);

    let report = case
        .with_conn(|connection| {
            scan_case(connection, &case.case_root, &case.meta.id.0)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    assert_eq!(report.disposition, RecoveryDisposition::Ready, "{report:?}");
    assert!(report.items.iter().any(|item| {
        item.operation_id == "source-import:attempt-1"
            && item.disposition == RecoveryDisposition::Ready
            && item.state == Some(persistence_source::fjall::RecoveryState::Published)
    }));
    assert!(!layout.build_path().exists());
    assert!(layout.published_path().is_dir());

    case.with_conn(|connection| {
        let operation = StorageControlRepo::new(connection)
            .find_operation("source-import:attempt-1")?
            .unwrap();
        assert_eq!(operation.state, StorageOperationState::Published);
        assert_eq!(
            operation.expected_batch_digest,
            Some(hex::encode(projection_receipt.batch_digest))
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn startup_recovery_reports_a_missing_generation_seal_without_aborting() {
    let root = tempfile::tempdir().unwrap();
    let case = app_services::case_service::create_case(root.path(), "Missing Seal", None).unwrap();
    let source_id = DataSourceId("missing-seal-source".to_owned());
    let source = DataSource {
        id: source_id.clone(),
        name: "Missing seal source".to_owned(),
        kind: DataSourceKind::E01,
        source_path: root.path().join("source.E01"),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source_id.0, Some("linux"), None);
    storage.import_state = "importing".to_owned();
    case.with_conn(|connection| {
        DataSourceRepo::new(connection).insert_with_storage(&case.meta.id, &source, &storage)?;
        StorageControlRepo::new(connection).insert_operation(&StorageOperationRecord {
            operation_id: "source-import:missing-seal-attempt".to_owned(),
            case_id: case.meta.id.0.clone(),
            source_id: Some(source_id.0.clone()),
            attempt_id: "missing-seal-attempt".to_owned(),
            operation_kind: "source_import".to_owned(),
            state: StorageOperationState::Running,
            input_digest: None,
            expected_batch_digest: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            last_error_code: None,
        })
    })
    .unwrap();

    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        &case.case_root,
        &source_id.0,
        "missing-seal-attempt",
    )
    .unwrap();
    std::fs::create_dir_all(layout.build_path()).unwrap();
    let build = persistence_source::fjall::SourceStore::open(
        layout.build_path(),
        persistence_source::fjall::DatabaseConfig::default(),
    )
    .unwrap();
    let receipt = build
        .database()
        .write_batch(
            &[persistence_source::fjall::WriteAction::Put {
                keyspace: persistence_source::fjall::KeyspaceName::Meta,
                key: b"projection-complete".to_vec(),
                value: b"source-import".to_vec(),
            }],
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .unwrap();
    persistence_source::fjall::seal_source_generation_with_control(
        &build,
        &source_id.0,
        "missing-seal-attempt",
        &hex::encode(receipt.batch_digest),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    drop(build);
    std::fs::remove_file(
        layout
            .build_path()
            .join(persistence_source::fjall::SOURCE_GENERATION_SEAL_FILE),
    )
    .unwrap();

    let report = case
        .with_conn(|connection| {
            scan_case(connection, &case.case_root, &case.meta.id.0)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    assert_eq!(report.disposition, RecoveryDisposition::RecoveryRequired);
    assert!(report.items.iter().any(|item| {
        item.operation_id == "source-import:missing-seal-attempt"
            && item.disposition == RecoveryDisposition::RecoveryRequired
            && item.state == Some(persistence_source::fjall::RecoveryState::MissingSeal)
    }));
}
