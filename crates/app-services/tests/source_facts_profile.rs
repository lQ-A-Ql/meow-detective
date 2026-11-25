use app_services::{
    case_service,
    import_analysis::ImportAnalysisMode,
    import_job_coordinator::{run_single_import, SingleImportRequest},
    import_precheck, post_import_service, source_db,
    task_runtime::TaskManager,
};
use domain::DataSourcePlatform;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

#[test]
fn managed_linux_artifact_task_publishes_evidence_facts_after_catalog_import() {
    let root = tempfile::tempdir().unwrap();
    let evidence = root.path().join("evidence");
    std::fs::create_dir_all(evidence.join("etc")).unwrap();
    std::fs::write(evidence.join("etc/hostname"), b"fixture-host\n").unwrap();
    std::fs::write(
        evidence.join("etc/os-release"),
        b"NAME=FixtureLinux\nVERSION_ID=1\n",
    )
    .unwrap();
    let active = case_service::create_case(root.path(), "Linux facts", None).unwrap();
    let source_id = active
        .with_conn(|connection| {
            let job = JobRepo::new(connection).create(&active.meta.id.0, "facts import")?;
            let config = import_precheck::prepare_import_source_config_from_path(
                &evidence.to_string_lossy(),
                DataSourcePlatform::Linux,
            )
            .unwrap();
            run_single_import(SingleImportRequest {
                connection,
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
            Ok(DataSourceRepo::new(connection)
                .find_by_case(&active.meta.id)?
                .remove(0)
                .id)
        })
        .unwrap();
    let runtime = post_import_service::PostImportRuntime {
        tasks: Arc::new(TaskManager::new()),
        bitlocker: Arc::new(app_services::bitlocker_runtime::BitLockerUnlockRegistry::default()),
        events: None,
    };
    assert_eq!(
        post_import_service::enqueue_case_pending(&runtime, &active.meta.id, &active.case_root,)
            .unwrap(),
        2
    );
    for (_, result) in runtime.tasks.wait_all(Duration::from_secs(30)) {
        result.unwrap();
    }
    active
        .with_conn(|connection| {
            let source = source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                &source_id,
            )?;
            let facts = analysis_core::linux_evidence_facts::load_persisted_linux_evidence_facts(
                &source, &source_id,
            )
            .unwrap();
            assert_eq!(facts.hostname.as_deref(), Some("fixture-host"));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        std::fs::read(evidence.join("etc/hostname")).unwrap(),
        b"fixture-host\n"
    );
}
