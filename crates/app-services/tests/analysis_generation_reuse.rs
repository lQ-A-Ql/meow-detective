use app_services::{
    analysis_service, case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_with_counts, ImportJobOptions},
    import_precheck, source_db,
};
use domain::DataSourcePlatform;
use persistence_source::fjall::instrumentation;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

#[test]
fn unchanged_real_linux_analysis_preserves_generation_without_copy_or_writes() {
    let scratch =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/repair-2026-11-13/analysis/tests");
    std::fs::create_dir_all(&scratch).unwrap();
    let directory = tempfile::tempdir_in(scratch).unwrap();
    let evidence = directory.path().join("evidence/home/investigator");
    std::fs::create_dir_all(&evidence).unwrap();
    std::fs::write(evidence.join(".bash_history"), b"pwd\nls -la\n").unwrap();
    let active =
        case_service::create_case(&directory.path().join("cases"), "generation-reuse", None)
            .unwrap();
    active
        .with_conn(|connection| {
            let job = JobRepo::new(connection).create(&active.meta.id.0, "reuse import")?;
            JobRepo::new(connection).start(&job)?;
            let config = import_precheck::prepare_import_source_config_from_path(
                directory.path().join("evidence").to_str().unwrap(),
                DataSourcePlatform::Linux,
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
                    defer_linux_artifacts: false,
                },
            )
            .unwrap();
            JobRepo::new(connection).complete_committed(&job, &message)?;
            let source_id = DataSourceRepo::new(connection)
                .find_by_case(&active.meta.id)?
                .remove(0)
                .id;
            let source = source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                &source_id,
            )?;
            let before: String = source.query_row(
                "SELECT value FROM source_meta WHERE key='fjall_root'",
                [],
                |row| row.get(0),
            )?;
            drop(source);
            instrumentation::enable();
            instrumentation::reset();
            let run = analysis_service::run_source_analysis_extraction_execution_with_cancel(
                connection,
                &active.case_root,
                &active.meta.id,
                &source_id,
                &["LinuxArtifacts"],
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            let counters = instrumentation::snapshot();
            let source = source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                &source_id,
            )?;
            let after: String = source.query_row(
                "SELECT value FROM source_meta WHERE key='fjall_root'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(
                before, after,
                "replay outcome: {:?}; work: {:?}",
                run.dto, run.work_metrics
            );
            assert!(run.dto.checkpoint_hit_count >= 1);
            assert_eq!(counters.write_batches, 0);
            assert_eq!(counters.bytes_written, 0);
            assert!(!run
                .work_metrics
                .iter()
                .any(|work| work.stage == transport::dto::AnalysisWorkStageDto::GenerationClone));
            Ok(())
        })
        .unwrap();
}
