use app_services::{
    analysis_service, case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_with_counts, ImportJobOptions},
    import_precheck, source_db,
};
use domain::{DataSourceId, DataSourcePlatform};
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
            let (message, counts) = execute_import_job_with_counts(
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
            assert_eq!(
                counts.failed_count, 0,
                "initial analysis must create reusable output"
            );
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
            let runtime = analysis_service::AnalysisSourceReadRuntime::default();
            let mut updates = Vec::new();
            let run = analysis_service::run_import_source_extraction(
                analysis_service::ImportSourceExtractionRequest {
                    case_conn: connection,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                    source: &source_id,
                    categories: &["LinuxArtifacts"],
                    runtime: &runtime,
                    cancel_token: Arc::new(AtomicBool::new(false)),
                    run_id: "reuse-timing",
                },
                |update| updates.push(update),
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
            assert_completion_timing(&run, &updates);
            Ok(())
        })
        .unwrap();
}

fn assert_completion_timing(
    run: &analysis_service::AnalysisExtractionExecution,
    updates: &[transport::dto::AnalysisExtractionProgressDto],
) {
    use transport::dto::{AnalysisExtractionPhaseDto, AnalysisWorkStageDto};
    let stages = [
        AnalysisWorkStageDto::ResultBuilding,
        AnalysisWorkStageDto::Audit,
        AnalysisWorkStageDto::LocatorFlush,
        AnalysisWorkStageDto::Completion,
    ];
    let terminal = updates
        .iter()
        .position(|update| {
            matches!(
                update.phase,
                AnalysisExtractionPhaseDto::Completed | AnalysisExtractionPhaseDto::Failed
            )
        })
        .unwrap();
    let mut previous = 0;
    for stage in stages {
        let position = updates
            .iter()
            .position(|update| {
                update
                    .work
                    .as_ref()
                    .is_some_and(|work| work.stage == stage && work.complete)
            })
            .unwrap();
        assert!(position >= previous && position < terminal);
        previous = position;
    }
    let measured = run
        .work_metrics
        .iter()
        .filter(|work| stages.contains(&work.stage))
        .map(|work| work.elapsed_ms)
        .sum::<u64>();
    assert!(run.persistence_elapsed_ms >= measured);
    assert!(updates[terminal..]
        .iter()
        .all(|update| update.work.is_none()));
    let mut completed = std::collections::BTreeSet::new();
    for update in &updates[terminal..] {
        assert!(
            completed.insert(update.category.clone()),
            "duplicate terminal category"
        );
    }
}

fn unanalyzed_source() -> (
    tempfile::TempDir,
    app_services::active_case::ActiveCase,
    DataSourceId,
) {
    let scratch =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/repair-2026-11-13/analysis/tests");
    std::fs::create_dir_all(&scratch).unwrap();
    let directory = tempfile::tempdir_in(scratch).unwrap();
    let evidence = directory.path().join("evidence/home/investigator");
    std::fs::create_dir_all(&evidence).unwrap();
    std::fs::write(evidence.join(".bash_history"), b"pwd\nls\n").unwrap();
    let active =
        case_service::create_case(&directory.path().join("cases"), "terminal-progress", None)
            .unwrap();
    let source = active
        .with_conn(|connection| {
            let job = JobRepo::new(connection).create(&active.meta.id.0, "metadata import")?;
            JobRepo::new(connection).start(&job)?;
            let config = import_precheck::prepare_import_source_config_from_path(
                directory.path().join("evidence").to_str().unwrap(),
                DataSourcePlatform::Linux,
            )
            .unwrap();
            let cancel = Arc::new(AtomicBool::new(false));
            let (message, counts) = execute_import_job_with_counts(
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
            assert_eq!(counts.failed_count, 0);
            JobRepo::new(connection).complete_committed(&job, &message)?;
            Ok(DataSourceRepo::new(connection)
                .find_by_case(&active.meta.id)?
                .remove(0)
                .id)
        })
        .unwrap();
    (directory, active, source)
}

fn binding(active: &app_services::active_case::ActiveCase, source: &DataSourceId) -> String {
    active
        .with_conn(|connection| {
            let source = source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                source,
            )?;
            source
                .query_row(
                    "SELECT value FROM source_meta WHERE key='fjall_root'",
                    [],
                    |row| row.get(0),
                )
                .map_err(persistence_sqlite::DbError::from)
        })
        .unwrap()
}

#[test]
fn cancelled_real_analysis_never_emits_a_buffered_completion() {
    use transport::dto::{AnalysisExtractionPhaseDto, AnalysisWorkStageDto};
    let (_directory, active, source) = unanalyzed_source();
    let before = binding(&active, &source);
    let cancel = Arc::new(AtomicBool::new(false));
    let runtime = analysis_service::AnalysisSourceReadRuntime::default();
    let mut updates = Vec::new();
    let result = active
        .with_conn(|connection| {
            Ok(analysis_service::run_import_source_extraction(
                analysis_service::ImportSourceExtractionRequest {
                    case_conn: connection,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                    source: &source,
                    categories: &["LinuxArtifacts"],
                    runtime: &runtime,
                    cancel_token: cancel.clone(),
                    run_id: "cancel-terminal",
                },
                |update| {
                    if update.work.as_ref().is_some_and(|work| {
                        work.stage == AnalysisWorkStageDto::GenerationClone && work.complete
                    }) {
                        cancel.store(true, std::sync::atomic::Ordering::Release);
                    }
                    updates.push(update);
                },
            ))
        })
        .unwrap();
    assert!(matches!(
        result,
        Err(analysis_service::AnalysisServiceError::Cancelled)
    ));
    assert_eq!(binding(&active, &source), before);
    assert!(updates
        .iter()
        .all(|update| update.phase != AnalysisExtractionPhaseDto::Completed));
}

#[test]
fn failed_real_source_read_has_one_failed_terminal_per_category() {
    use transport::dto::AnalysisExtractionPhaseDto;
    let (directory, active, source) = unanalyzed_source();
    std::fs::remove_file(
        directory
            .path()
            .join("evidence/home/investigator/.bash_history"),
    )
    .unwrap();
    let runtime = analysis_service::AnalysisSourceReadRuntime::default();
    let mut updates = Vec::new();
    let run = active
        .with_conn(|connection| {
            Ok(analysis_service::run_import_source_extraction(
                analysis_service::ImportSourceExtractionRequest {
                    case_conn: connection,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                    source: &source,
                    categories: &["LinuxArtifacts"],
                    runtime: &runtime,
                    cancel_token: Arc::new(AtomicBool::new(false)),
                    run_id: "failed-terminal",
                },
                |update| updates.push(update),
            )
            .unwrap())
        })
        .unwrap();
    assert!(run.retryable_failure_count > 0);
    assert!(updates
        .iter()
        .all(|update| update.phase != AnalysisExtractionPhaseDto::Completed));
    let mut failed = std::collections::BTreeSet::new();
    for update in updates
        .iter()
        .filter(|update| update.phase == AnalysisExtractionPhaseDto::Failed)
    {
        assert!(failed.insert(update.category.clone()));
    }
    assert!(!failed.is_empty());
}
