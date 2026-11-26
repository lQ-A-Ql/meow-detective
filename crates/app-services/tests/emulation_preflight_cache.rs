use app_services::{
    case_service,
    import_analysis::ImportAnalysisMode,
    import_job_coordinator::{run_single_import, SingleImportRequest},
    import_precheck, mount_service,
};
use domain::DataSourcePlatform;
use persistence_source::fjall::instrumentation::ProcessMeasurement;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::sync::{atomic::AtomicBool, Arc};

#[test]
fn fresh_ext4_image_cache_rejects_changed_or_missing_sources() {
    let root = tempfile::tempdir().unwrap();
    let image = root.path().join("image.raw");
    let bytes = testing::builders::ext4::linux_root_ext4_image();
    std::fs::write(&image, &bytes).unwrap();
    let active = case_service::create_case(root.path(), "preflight-cache", None).unwrap();
    let conn = active.connection().unwrap();
    let job = JobRepo::new(&conn)
        .create(&active.meta.id.0, "Ext4 import")
        .unwrap();
    let config = import_precheck::prepare_import_source_config_from_path(
        image.to_str().unwrap(),
        DataSourcePlatform::Linux,
    )
    .unwrap();
    run_single_import(SingleImportRequest {
        connection: &conn,
        case_id: &active.meta.id,
        case_root: &active.case_root,
        job_id: &job,
        import_config: config,
        max_import_workers: Some(1),
        max_analysis_workers: Some(1),
        analysis_mode: ImportAnalysisMode::MetadataOnly,
        cancel_token: &Arc::new(AtomicBool::new(false)),
        event_sink: None,
    })
    .unwrap();
    let source = DataSourceRepo::new(&conn)
        .find_by_case(&active.meta.id)
        .unwrap()
        .remove(0)
        .id;
    let first =
        mount_service::emulation_preflight(&conn, &active.case_root, &active.meta.id, &source)
            .unwrap();
    let work = ProcessMeasurement::start().unwrap();
    let second =
        mount_service::emulation_preflight(&conn, &active.case_root, &active.meta.id, &source)
            .unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
    assert_eq!(work.snapshot().scan_calls, 0);
    drop(work);
    mount_service::clear_emulation_preflight_cache();
    let work = ProcessMeasurement::start().unwrap();
    mount_service::emulation_preflight(&conn, &active.case_root, &active.meta.id, &source).unwrap();
    assert!(
        work.snapshot().scan_calls > 0,
        "retirement invalidation requires a new inspection"
    );
    drop(work);
    std::fs::File::options()
        .write(true)
        .open(&image)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(1))
        .unwrap();
    let work = ProcessMeasurement::start().unwrap();
    mount_service::emulation_preflight(&conn, &active.case_root, &active.meta.id, &source).unwrap();
    assert!(
        work.snapshot().scan_calls > 0,
        "changed metadata requires a new inspection"
    );
    drop(work);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&image)
        .unwrap()
        .set_len(bytes.len() as u64 + 512)
        .unwrap();
    assert!(
        mount_service::emulation_preflight(&conn, &active.case_root, &active.meta.id, &source)
            .is_err()
    );
    std::fs::remove_file(&image).unwrap();
    assert!(
        mount_service::emulation_preflight(&conn, &active.case_root, &active.meta.id, &source)
            .is_err()
    );
}
