use super::*;
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};

fn case_with_evidence() -> (
    tempfile::TempDir,
    crate::active_case::ActiveCase,
    std::path::PathBuf,
) {
    let temporary = tempfile::TempDir::new().unwrap();
    let evidence = temporary.path().join("evidence");
    std::fs::create_dir_all(&evidence).unwrap();
    let active = crate::case_service::create_case(
        &temporary.path().join("cases"),
        "extraction-policy",
        Some("tester"),
    )
    .unwrap();
    active
        .with_conn(|conn| {
            let id = domain::DataSourceId("source-1".to_string());
            let source = domain::DataSource {
                id: id.clone(),
                name: "evidence".to_string(),
                kind: domain::DataSourceKind::LogicalDirectory,
                source_path: evidence.clone(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            };
            let mut storage = DataSourceStorage::source_db(&id.0, Some("windows"), None);
            storage.import_state = "ready".to_string();
            DataSourceRepo::new(conn).insert_with_storage(&active.meta.id, &source, &storage)
        })
        .unwrap();
    (temporary, active, evidence)
}

#[test]
fn external_export_rejects_case_workspace_and_evidence_tree() {
    let (_temporary, active, evidence) = case_with_evidence();
    active
        .with_conn(|conn| {
            let case_target = active.case_root.join("reports").join("file.bin");
            let case_error = prepare_destination(
                &case_target,
                false,
                DestinationScope::ExternalCase {
                    case_conn: conn,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                },
            )
            .unwrap_err();
            assert!(matches!(case_error, FileServiceError::Security(_)));

            let evidence_target = evidence.join("file.bin");
            let evidence_error = prepare_destination(
                &evidence_target,
                false,
                DestinationScope::ExternalCase {
                    case_conn: conn,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                },
            )
            .unwrap_err();
            assert!(matches!(evidence_error, FileServiceError::Security(_)));
            Ok(())
        })
        .unwrap();
}

#[test]
fn managed_export_stays_inside_case_and_external_export_stays_outside() {
    let (temporary, active, _evidence) = case_with_evidence();
    active
        .with_conn(|conn| {
            let managed_target = active.case_root.join("reports").join("bundle.bin");
            let prepared = prepare_destination(
                &managed_target,
                false,
                DestinationScope::CaseManaged {
                    case_conn: conn,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                },
            )
            .unwrap();
            assert!(prepared.starts_with(active.case_root.canonicalize().unwrap()));

            let external_target = temporary.path().join("exports").join("file.bin");
            let prepared = prepare_destination(
                &external_target,
                false,
                DestinationScope::ExternalCase {
                    case_conn: conn,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                },
            )
            .unwrap();
            assert!(prepared.starts_with(temporary.path().canonicalize().unwrap()));
            Ok(())
        })
        .unwrap();
}

#[test]
fn relative_and_symlink_destinations_are_rejected() {
    let relative = prepare_destination(
        std::path::Path::new("relative.bin"),
        false,
        DestinationScope::Unscoped,
    )
    .unwrap_err();
    assert!(matches!(relative, FileServiceError::InvalidInput(_)));

    #[cfg(unix)]
    {
        let temporary = tempfile::TempDir::new().unwrap();
        let real = temporary.path().join("real.bin");
        let link = temporary.path().join("link.bin");
        std::fs::write(&real, b"data").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let error = prepare_destination(&link, true, DestinationScope::Unscoped).unwrap_err();
        assert!(matches!(error, FileServiceError::Security(_)));
    }
}

#[cfg(windows)]
#[test]
fn windows_alternate_data_stream_destination_is_rejected() {
    let error = prepare_destination(
        std::path::Path::new(r"C:\evidence.E01:export"),
        true,
        DestinationScope::Unscoped,
    )
    .unwrap_err();

    assert!(matches!(error, FileServiceError::Security(_)));
}
