use app_services::{
    case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_with_counts, ImportJobOptions},
    import_precheck, source_db,
};
use domain::DataSourcePlatform;
use persistence_source::fjall::{clone_verified_generation_with_control, DatabaseConfig};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

#[test]
fn reopening_a_real_case_discovers_retired_generations_without_a_prior_gc_request() {
    let scratch =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/repair-2026-11-13/analysis/tests");
    std::fs::create_dir_all(&scratch).unwrap();
    let directory = tempfile::tempdir_in(scratch).unwrap();
    let evidence = directory.path().join("evidence");
    std::fs::create_dir(&evidence).unwrap();
    std::fs::write(evidence.join("notes.txt"), b"case opening regression\n").unwrap();
    let active =
        case_service::create_case(&directory.path().join("cases"), "case-gc", None).unwrap();
    let case_root = active.case_root.clone();
    let (current, old) = active
        .with_conn(|connection| {
            let job = JobRepo::new(connection).create(&active.meta.id.0, "case-gc import")?;
            JobRepo::new(connection).start(&job)?;
            let config = import_precheck::prepare_import_source_config_from_path(
                evidence.to_str().unwrap(),
                DataSourcePlatform::Windows,
            )
            .unwrap();
            let cancel = Arc::new(AtomicBool::new(false));
            let (message, _) = execute_import_job_with_counts(
                connection,
                &active.meta.id,
                &active.case_root,
                config,
                &job,
                ImportJobOptions {
                    projection_policy: Default::default(),
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: Some(2),
                    max_analysis_workers: Some(2),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: true,
                },
            )
            .unwrap();
            JobRepo::new(connection).complete_committed(&job, &message)?;
            let source = DataSourceRepo::new(connection)
                .find_by_case(&active.meta.id)?
                .remove(0);
            let sql = source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                &source.id,
            )?;
            let store = persistence_source::detail::require_source_store(&sql)?;
            let current = store.database().path().to_owned();
            let attempt = format!("analysis-{}", uuid::Uuid::new_v4());
            let old = current
                .parent()
                .unwrap()
                .join(format!("source.fjall.build.{attempt}"));
            let (copy, verified) = clone_verified_generation_with_control(
                &store,
                &source.id.0,
                &old,
                DatabaseConfig::default(),
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap();
            verified
                .seal(&copy, &attempt, &AtomicBool::new(false))
                .unwrap();
            Ok((current, old))
        })
        .unwrap();
    drop(active);
    assert!(old.is_dir());
    let reopened = case_service::open_case(&case_root).unwrap();
    persistence_source::publication::drain_analysis_generation_gc(Duration::from_secs(5)).unwrap();
    assert!(!old.exists());
    assert!(current.is_dir());
    assert_eq!(
        reopened.case_root.canonicalize().unwrap(),
        case_root.canonicalize().unwrap()
    );
}
