#[path = "support/evidence_case.rs"]
mod fixture;
use app_services::{
    import_pipeline::{execute_import_job, ImportJobOptions},
    import_precheck::prepare_import_source_config_from_path,
    nested_evidence_service,
};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::sync::atomic::{AtomicBool, Ordering};

fn vhd_bytes() -> Vec<u8> {
    let mut bytes = testing::builders::ext4::minimal_ext4_image();
    let size = bytes.len() as u64;
    let mut footer = [0u8; 512];
    footer[..8].copy_from_slice(b"conectix");
    footer[12..16].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    footer[16..24].copy_from_slice(&u64::MAX.to_be_bytes());
    footer[40..48].copy_from_slice(&size.to_be_bytes());
    footer[48..56].copy_from_slice(&size.to_be_bytes());
    footer[60..64].copy_from_slice(&2u32.to_be_bytes());
    let sum: u32 = footer.iter().map(|byte| u32::from(*byte)).sum();
    footer[64..68].copy_from_slice(&(!sum).to_be_bytes());
    bytes.extend_from_slice(&footer);
    bytes
}
fn setup() -> fixture::EvidenceCase {
    let case = fixture::EvidenceCase::new();
    let evidence = case.temp.path().join("nested-parent");
    std::fs::create_dir(&evidence).unwrap();
    let bytes = vhd_bytes();
    std::fs::write(evidence.join("child.vhd"), &bytes).unwrap();
    case.active
        .with_conn(|conn| {
            let source = case.source(
                conn,
                "parent",
                &evidence,
                domain::DataSourceKind::LogicalDirectory,
            );
            fixture::entry(&source, "parent", "child", "child.vhd", bytes.len() as u64);
            Ok(())
        })
        .unwrap();
    case
}
#[test]
fn nested_pipeline_reuses_registered_id_and_exposes_child_catalog_and_bytes() {
    let case = setup();
    case.active
        .with_conn(|conn| {
            let revision = parent_revision(conn, &case);
            let prepared = materialize(conn, &case, &revision).unwrap();
            assert!(prepared
                .probe
                .as_ref()
                .is_some_and(|probe| !probe.partitions.is_empty()));
            let result = prepared.result;
            let prepared_probe = prepared.probe;
            let repeated = materialize(conn, &case, &revision).unwrap();
            assert_eq!(
                result.derived_data_source_id,
                repeated.result.derived_data_source_id
            );
            assert_eq!(result.lineage.parent_file_id.as_deref(), Some("child"));
            let source = DataSourceRepo::new(conn)
                .find_by_case(&case.active.meta.id)?
                .into_iter()
                .find(|source| source.id.0 == result.derived_data_source_id)
                .unwrap();
            let job = JobRepo::new(conn).create(&case.active.meta.id.0, "nestedImport")?;
            let mut config = prepare_import_source_config_from_path(
                &source.source_path.to_string_lossy(),
                domain::DataSourcePlatform::Linux,
            )
            .unwrap();
            config.existing_data_source_id = Some(source.id.clone());
            config.prepared_probe = prepared_probe;
            execute_import_job(
                conn,
                &case.active.meta.id,
                &case.active.case_root,
                config,
                &job,
                ImportJobOptions {
                    projection_policy: Default::default(),
                    event_sink: None,
                    cancel_token: &std::sync::Arc::new(AtomicBool::new(false)),
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: app_services::import_analysis::ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: false,
                },
            )
            .unwrap();
            assert_eq!(
                DataSourceRepo::new(conn)
                    .find_by_case(&case.active.meta.id)?
                    .len(),
                2
            );
            assert_eq!(
                DataSourceRepo::new(conn)
                    .find_storage(&source.id)?
                    .unwrap()
                    .import_state,
                "ready"
            );
            let ready = app_services::source_db::open_ready_source_read_only_by_id(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &source.id,
            )
            .unwrap();
            let files = FileRepo::new(&ready.connection).find_by_path_prefix(&source.id, "")?;
            let file = files
                .iter()
                .find(|file| file.name == "test.txt")
                .expect("browsable nested ext4 file");
            let file_id = app_services::source_db::encode_source_scoped_id(&source.id, &file.id.0);
            let handle = app_services::file_service::open_file_handle_for_case(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &file_id,
            )
            .unwrap();
            let range = app_services::file_service::read_file_range_for_source_case(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &transport::dto::ViewerRangeRequestDto {
                    handle_id: handle.handle_id,
                    offset: 0,
                    length: 11,
                },
            )
            .unwrap();
            let bytes = range.raw_bytes.unwrap();
            assert_eq!(bytes, b"Hello World");
            assert_eq!(
                nested_evidence_service::list_for_parent(
                    conn,
                    &case.active.case_root,
                    &case.active.meta.id,
                    "parent",
                )
                .unwrap()[0]
                    .derived_data_source_id
                    .as_deref(),
                Some(source.id.0.as_str())
            );
            assert_eq!(
                std::fs::read(case.temp.path().join("nested-parent/child.vhd")).unwrap(),
                vhd_bytes()
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn mid_copy_cancel_leaves_no_derived_source_and_retry_succeeds() {
    let case = setup();
    case.active
        .with_conn(|conn| {
            let cancel = AtomicBool::new(false);
            let revision = parent_revision(conn, &case);
            let result = nested_evidence_service::materialize_fixed_vhd_with_cancel_prepared(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                "parent",
                "child",
                &revision,
                &cancel,
                &|_, _| cancel.store(true, Ordering::Release),
            );
            assert!(matches!(
                result,
                Err(nested_evidence_service::NestedEvidenceError::Cancelled)
            ));
            assert_eq!(
                DataSourceRepo::new(conn)
                    .find_by_case(&case.active.meta.id)?
                    .len(),
                1
            );
            assert_eq!(
                std::fs::read_dir(case.active.case_root.join("derived/nested-evidence"))
                    .unwrap()
                    .count(),
                0
            );
            assert!(materialize(conn, &case, &revision).is_ok());
            Ok(())
        })
        .unwrap();
}
#[test]
fn changed_materialized_bytes_are_rejected_on_retry() {
    let case = setup();
    case.active
        .with_conn(|conn| {
            let revision = parent_revision(conn, &case);
            let result = materialize(conn, &case, &revision).unwrap();
            let path = case
                .active
                .case_root
                .join("derived/nested-evidence")
                .join(format!("{}.vhd", result.result.derived_data_source_id));
            let mut bytes = std::fs::read(&path).unwrap();
            bytes[0] ^= 1;
            std::fs::write(path, bytes).unwrap();
            assert!(materialize(conn, &case, &revision).is_err());
            Ok(())
        })
        .unwrap();
}

#[test]
fn changed_parent_revision_rejects_reuse() {
    let case = setup();
    case.active
        .with_conn(|conn| {
            let revision = parent_revision(conn, &case);
            materialize(conn, &case, &revision).unwrap();
            let path = case.temp.path().join("nested-parent/child.vhd");
            let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
            file.set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
            )
            .unwrap();
            assert_ne!(parent_revision(conn, &case), revision);
            assert!(matches!(
                materialize(conn, &case, &revision),
                Err(nested_evidence_service::NestedEvidenceError::Materialization(
                    nested_evidence_service::MaterializeError::Source(message)
                )) if message == "parent evidence changed before nested evidence reuse"
            ));
            Ok(())
        })
        .unwrap();
}

fn materialize(
    conn: &rusqlite::Connection,
    case: &fixture::EvidenceCase,
    revision: &str,
) -> Result<
    nested_evidence_service::NestedEvidenceMaterialized,
    nested_evidence_service::NestedEvidenceError,
> {
    nested_evidence_service::materialize_fixed_vhd_with_cancel_prepared(
        conn,
        &case.active.case_root,
        &case.active.meta.id,
        "parent",
        "child",
        revision,
        &AtomicBool::new(false),
        &|_, _| {},
    )
}

fn parent_revision(conn: &rusqlite::Connection, case: &fixture::EvidenceCase) -> String {
    app_services::digest_service::identity::snapshot(
        conn,
        &case.active.case_root,
        &case.active.meta.id,
        &app_services::digest_service::DigestTarget {
            data_source_id: Some("parent"),
            file_id: Some("ds:parent:child"),
            partition_index: None,
        },
    )
    .unwrap()
    .revision
}
