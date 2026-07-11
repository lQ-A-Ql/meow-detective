#[allow(dead_code)]
mod fixture_builder;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use app_services::import_analysis::{
    content_budget_for_mode, default_memory_hard_limit_mb, default_memory_soft_limit_mb,
    run_import_analysis_staging, ImportAnalysisError, ImportAnalysisMode, ImportAnalysisOptions,
};
use app_services::import_pipeline::{execute_import_job, ImportJobOptions};
use app_services::{case_service, import_precheck, search_service, source_db};
use domain::{DataSourceId, DataSourcePlatform};
use fixture_builder::build_prefetch_v30;
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, job_repo::JobRepo};
use tempfile::TempDir;
use transport::{ErrorCategory, ServiceErrorCategory};

#[derive(Debug, PartialEq, Eq)]
struct PipelineOutcome {
    artifact_count: i64,
    timeline_count: i64,
    indexed_marker_count: u64,
}

#[test]
fn post_import_registry_is_platform_specific_and_shared_analysis_remains_enabled() {
    let windows = run_logical_import(DataSourcePlatform::Windows, "windows-post-import");
    assert!(windows.artifact_count > 0);
    assert!(windows.timeline_count > 0);
    assert_eq!(windows.indexed_marker_count, 1);

    let linux = run_logical_import(DataSourcePlatform::Linux, "linux-post-import");
    assert_eq!(linux.artifact_count, 0);
    assert!(linux.timeline_count > 0);
    assert_eq!(linux.indexed_marker_count, 1);
}

#[test]
fn unknown_platform_fails_before_database_or_staging_access() {
    let tmp = TempDir::new().expect("temp directory");
    let case_root = tmp.path().join("case-root-must-not-be-created");
    let db_path = tmp.path().join("source-must-not-be-created.db");
    let mode = ImportAnalysisMode::BudgetedContent;
    let error = run_import_analysis_staging(
        ImportAnalysisOptions {
            case_root: case_root.clone(),
            db_path: db_path.clone(),
            case_id: "case-unknown".to_string(),
            data_source_id: DataSourceId("ds-unknown".to_string()),
            platform: DataSourcePlatform::Unknown,
            index_dir: tmp.path().join("index-must-not-be-created"),
            max_analysis_workers: Some(1),
            cancel_token: Arc::new(AtomicBool::new(false)),
            enable_timeline_projection: true,
            enable_content_extraction: true,
            enable_text_indexing: true,
            analysis_mode: mode,
            content_budget: content_budget_for_mode(mode),
            memory_soft_limit_mb: default_memory_soft_limit_mb(),
            memory_hard_limit_mb: default_memory_hard_limit_mb(),
            tier_state: Arc::new(Mutex::new(
                app_services::import_analysis::tier::TierStateMachine::new(),
            )),
        },
        None,
    )
    .expect_err("unknown platform must fail closed");

    assert!(matches!(
        error,
        ImportAnalysisError::UnsupportedPlatform(ref value) if value == "unknown"
    ));
    assert!(matches!(error.category(), ErrorCategory::Unsupported));
    assert!(!db_path.exists());
    assert!(!case_root.exists());
}

fn run_logical_import(platform: DataSourcePlatform, case_name: &str) -> PipelineOutcome {
    let tmp = TempDir::new().expect("temp directory");
    let evidence_dir = tmp.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).expect("create evidence directory");
    let marker = format!("platform-policy-marker-{case_name}");
    std::fs::write(evidence_dir.join("notes.txt"), &marker).expect("write text fixture");
    std::fs::write(
        evidence_dir.join("CMD.EXE-12345678.pf"),
        build_prefetch_v30("CMD.EXE", 3, &[chrono::Utc::now()]),
    )
    .expect("write Prefetch fixture");

    let active = case_service::create_case(&tmp.path().join("cases"), case_name, Some("tester"))
        .expect("create case");
    let cancel = Arc::new(AtomicBool::new(false));
    active
        .with_conn(|conn| {
            let job_id = JobRepo::new(conn).create(&active.meta.id.0, "Platform policy import")?;
            let config = import_precheck::prepare_import_source_config_from_path(
                &evidence_dir.to_string_lossy(),
                platform,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            execute_import_job(
                conn,
                &active.meta.id,
                &active.case_root,
                config,
                &job_id,
                ImportJobOptions {
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::BudgetedContent,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

            let data_source_id = conn.query_row(
                "SELECT id FROM data_sources WHERE case_id = ?1 ORDER BY imported_at DESC LIMIT 1",
                [&active.meta.id.0],
                |row| row.get::<_, String>(0).map(DataSourceId),
            )?;
            let source_conn = source_db::open_source_db(&active.case_root, &data_source_id)?;
            let artifact_count = ArtifactRepo::new(&source_conn).count()? as i64;
            let timeline_count =
                source_conn
                    .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))?;
            let search_result = search_service::search_files_real(
                &source_db::source_index_dir(&active.case_root, &data_source_id),
                &marker,
                0,
                10,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            Ok(PipelineOutcome {
                artifact_count,
                timeline_count,
                indexed_marker_count: search_result.total,
            })
        })
        .expect("run platform-specific import")
}
