use super::*;

#[test]
fn rollback_surfaces_tombstone_cleanup_failure_after_restoring_paths() {
    let temp = tempfile::TempDir::new().unwrap();
    let original = temp.path().join("source");
    let tombstone_root = temp.path().join("tombstone");
    let staged_path = tombstone_root.join("source");
    std::fs::create_dir_all(&staged_path).unwrap();
    std::fs::write(staged_path.join("marker"), b"restorable").unwrap();
    std::fs::write(tombstone_root.join("cleanup-blocker"), b"block remove_dir").unwrap();
    let staged = [StagedDataSourcePath {
        label: "source",
        original: original.clone(),
        tombstone: staged_path,
    }];

    let error = rollback_staged_deletion(
        "ds-rollback-cleanup",
        &tombstone_root,
        "cache/data-source-tombstones/ds-rollback-cleanup",
        &staged,
        "database transaction failed".to_string(),
        CaseServiceError::InvalidCaseDir("primary failure".to_string()),
    );

    match error {
        CaseServiceError::DataSourceDeleteRollbackFailed {
            data_source_id,
            tombstone,
            step,
            original,
            rollback,
        } => {
            assert_eq!(data_source_id, "ds-rollback-cleanup");
            assert_eq!(
                tombstone,
                "cache/data-source-tombstones/ds-rollback-cleanup"
            );
            assert_eq!(step, "cleanupEmptyTombstone");
            assert!(matches!(
                *original,
                CaseServiceError::InvalidCaseDir(ref message) if message == "primary failure"
            ));
            assert_eq!(rollback.kind(), std::io::ErrorKind::DirectoryNotEmpty);
            assert!(rollback.to_string().contains("database transaction failed"));
            assert!(rollback.to_string().contains("cleanup failed"));
        }
        other => panic!("unexpected rollback error: {other:?}"),
    }
    assert_eq!(
        std::fs::read(original.join("marker")).unwrap(),
        b"restorable"
    );
    assert!(tombstone_root.join("cleanup-blocker").is_file());
}

#[test]
fn deletes_ceph_rbd_managed_storage_without_opening_virtual_source_path() {
    let temp = tempfile::TempDir::new().unwrap();
    let case_root = temp.path().join("case");
    std::fs::create_dir_all(case_root.join("cache")).unwrap();
    let conn = persistence_sqlite::open_or_create(&case_root.join("app.db")).unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    let case = domain::CaseMeta {
        id: domain::CaseId("case-rbd-delete".to_string()),
        name: "RBD deletion".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn)
        .create(&case)
        .unwrap();

    let data_source_id = domain::DataSourceId("rbd-source".to_string());
    let data_source = domain::DataSource {
        id: data_source_id.clone(),
        name: "Derived VM disk".to_string(),
        kind: domain::DataSourceKind::CephRbd,
        source_path: std::path::PathBuf::from("ceph-rbd://cluster/image"),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    let storage = persistence_sqlite::repositories::datasource_repo::DataSourceStorage::source_db(
        &data_source_id.0,
        Some("linux"),
        Some("vm_disk".to_string()),
    );
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&conn)
        .insert_with_storage(&case.id, &data_source, &storage)
        .unwrap();

    let source_dir = crate::source_db::source_dir(&case_root, &data_source_id);
    std::fs::create_dir_all(source_dir.join("index")).unwrap();
    std::fs::write(source_dir.join("source.db"), b"managed").unwrap();
    std::fs::create_dir_all(
        crate::source_db::source_staging_dir(&case_root, &data_source_id).unwrap(),
    )
    .unwrap();

    delete_data_source_in(&conn, &case_root, &data_source_id.0).unwrap();

    assert!(
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&conn)
            .find_storage(&data_source_id)
            .unwrap()
            .is_none()
    );
    assert!(!source_dir.exists());
    assert!(!case_root.join("staging").join(&data_source_id.0).exists());
}

#[test]
fn gc_timeout_retains_source_registration_and_reports_io_error() {
    let workspace = tempfile::TempDir::new().unwrap();
    let case = crate::case_service::create_case(workspace.path(), "source-close", None).unwrap();
    let conn = case.connection().unwrap();
    let source_path = workspace.path().join("evidence.raw");
    std::fs::write(&source_path, [0; 512]).unwrap();
    let id = domain::DataSourceId("gc-close".into());
    let source = domain::DataSource {
        id: id.clone(),
        name: "source-close".into(),
        kind: domain::DataSourceKind::Raw,
        source_path: source_path.clone(),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    let repo = DataSourceRepo::new(&conn);
    repo.insert_with_storage(
        &case.meta.id,
        &source,
        &DataSourceStorage::source_db(&id.0, Some("linux"), None),
    )
    .unwrap();
    drop(crate::source_db::open_source_db(&case.case_root, &id).unwrap());
    let directory = crate::source_db::source_dir(&case.case_root, &id);
    let reclamation = persistence_source::control::connections::try_begin_generation_gc(&directory)
        .unwrap()
        .unwrap();
    let error = delete_data_source_in(&conn, &case.case_root, &id.0).unwrap_err();
    assert!(matches!(
        transport::ServiceErrorCategory::category(&error),
        transport::ErrorCategory::Io
    ));
    let CaseServiceError::Db(persistence_sqlite::DbError::Io(error)) = error else {
        panic!("expected source reclamation I/O error");
    };
    assert!(error
        .get_ref()
        .unwrap()
        .downcast_ref::<persistence_source::control::connections::SourceGenerationBusy>()
        .is_some());
    assert!(directory.is_dir());
    assert!(repo.find_storage(&id).unwrap().is_some());
    drop(reclamation);
    delete_data_source_in(&conn, &case.case_root, &id.0).unwrap();
    assert!(!directory.exists());
    assert!(repo.find_storage(&id).unwrap().is_none());
    assert_eq!(std::fs::read(source_path).unwrap(), [0; 512]);
}
