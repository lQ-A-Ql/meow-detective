//! Real-E01 acceptance for detached catalog publication and managed timeline work.
//! Set FORENSICS_E01_FIXTURE to an existing E01 filesystem image and run:
//!
//! ```text
//! $env:FORENSICS_E01_FIXTURE = "<path-to-real.e01>"
//! cargo test -p app-services --test capability_lifecycle_e2e -- --include-ignored --nocapture
//! ```
//!
//! Catalog/search are ready while timeline/artifacts remain queued. A managed
//! timeline request completes Timeline/Graph and leaves artifact work queued.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::import_analysis::ImportAnalysisMode;
use app_services::import_job_coordinator::{
    run_single_import, SingleImportOutcome, SingleImportRequest,
};
use app_services::post_import_service::{self, PostImportRuntime};
use app_services::task_runtime::TaskManager;
use domain::DataSourcePlatform;
use persistence_sqlite::repositories::job_repo::JobRepo;
use persistence_sqlite::DbError;
use transport::dto::{CapabilityKindDto, CapabilityStateDto};

fn matrix_state(
    sources: &[transport::dto::SourceCapabilityDto],
    source_id: &str,
    kind: CapabilityKindDto,
) -> CapabilityStateDto {
    sources
        .iter()
        .find(|row| row.data_source_id == source_id && row.capability == kind)
        .unwrap_or_else(|| panic!("missing {kind:?} row for {source_id}"))
        .state
}

#[test]
#[ignore = "requires a real E01 image; runs minutes; use --include-ignored"]
fn detached_import_lifecycle_capability_matrix() {
    let image = PathBuf::from(
        std::env::var_os("FORENSICS_E01_FIXTURE")
            .expect("FORENSICS_E01_FIXTURE must identify a real E01 image"),
    );
    assert!(image.is_file(), "configured E01 image must exist");
    let temp = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join("cases")).unwrap();
    let active = app_services::case_service::create_case(
        temp.path().join("cases").as_path(),
        "e2e-cap",
        Some("tester"),
    )
    .unwrap();

    let outcome = active
        .with_conn(|case_conn| {
            let config = app_services::import_precheck::prepare_import_source_config_from_path(
                &image.to_string_lossy(),
                DataSourcePlatform::Windows,
            )
            .map_err(|error| DbError::System(error.to_string()))?;
            let job_id =
                JobRepo::new(case_conn).create(&active.meta.id.0, "e2e detached import")?;
            let cancel = Arc::new(AtomicBool::new(false));
            let outcome = run_single_import(SingleImportRequest {
                connection: case_conn,
                case_id: &active.meta.id,
                case_root: &active.case_root,
                job_id: &job_id,
                import_config: config,
                max_import_workers: Some(1),
                max_analysis_workers: Some(1),
                analysis_mode: ImportAnalysisMode::MetadataOnly,
                cancel_token: &cancel,
                event_sink: None,
            })
            .map_err(|error| DbError::System(error.to_string()))?;

            let source_id: String = case_conn
                .query_row("SELECT id FROM data_sources LIMIT 1", [], |row| row.get(0))
                .map_err(|error| DbError::System(error.to_string()))?;

            let before = app_services::capability_service::derive_capability_matrix(
                case_conn,
                &active.case_root,
                &active.meta.id,
            )
            .map_err(|error| DbError::System(error.to_string()))?;
            assert_eq!(
                matrix_state(&before.sources, &source_id, CapabilityKindDto::Catalog),
                CapabilityStateDto::Ready,
                "catalog must be ready after detached import"
            );
            assert_eq!(
                matrix_state(&before.sources, &source_id, CapabilityKindDto::Search),
                CapabilityStateDto::Ready,
                "search mirrors catalog and must be ready"
            );
            assert_eq!(
                matrix_state(&before.sources, &source_id, CapabilityKindDto::Timeline),
                CapabilityStateDto::Queued,
                "timeline must be queued before its worker starts"
            );
            assert_eq!(
                matrix_state(&before.sources, &source_id, CapabilityKindDto::Artifacts),
                CapabilityStateDto::Queued,
                "detached artifact extraction must be queued"
            );

            let runtime = PostImportRuntime {
                tasks: Arc::new(TaskManager::new()),
                bitlocker: Arc::new(
                    app_services::bitlocker_runtime::BitLockerUnlockRegistry::default(),
                ),
                events: None,
            };
            post_import_service::rebuild_timeline(
                &runtime,
                &active.meta.id,
                &active.case_root,
                Some(&source_id),
            )
            .map_err(|error| DbError::System(error.to_string()))?;

            let after = app_services::capability_service::derive_capability_matrix(
                case_conn,
                &active.case_root,
                &active.meta.id,
            )
            .map_err(|error| DbError::System(error.to_string()))?;
            assert_eq!(
                matrix_state(&after.sources, &source_id, CapabilityKindDto::Timeline),
                CapabilityStateDto::Ready,
                "timeline must be ready after projection rebuild"
            );
            assert_eq!(
                matrix_state(&after.sources, &source_id, CapabilityKindDto::Graph),
                CapabilityStateDto::Ready,
                "graph mirrors timeline and must be ready"
            );
            assert_eq!(
                matrix_state(&after.sources, &source_id, CapabilityKindDto::Artifacts),
                CapabilityStateDto::Queued,
            );
            Ok(outcome)
        })
        .expect("detached import completes");

    match outcome {
        SingleImportOutcome::Completed(message) => {
            eprintln!("detached import completed: {message}");
        }
        SingleImportOutcome::Cancelled(reason) => {
            panic!("detached import was cancelled: {reason}");
        }
    }
}
