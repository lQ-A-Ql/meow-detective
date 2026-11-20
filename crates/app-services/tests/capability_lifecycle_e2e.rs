//! Ignored acceptance harness for the detached import capability lifecycle.
//!
//! Requires a real E01 image; synthetic tiny.E01 must NOT be used as a
//! filesystem sample. Run:
//!
//! ```text
//! $env:FORENSICS_E01_FIXTURE = "<path-to-real.e01>"
//! cargo test -p app-services --test capability_lifecycle_e2e -- --include-ignored --nocapture
//! ```
//!
//! Covered by this harness:
//! - `run_single_import_with_mode(Detached)` on a real E01 completes;
//! - pre-rebuild matrix: catalog/search ready, timeline pending, artifacts
//!   notApplicable under `MetadataOnly`;
//! - an explicit `rebuild_timeline_projection` moves timeline/graph to ready.
//!
//! Not covered here (design-only, requires dedicated real-sample runs):
//! - slow timeline buckets (500ms / 5s / 5min) timing measurement;
//! - process kill mid-import and resume re-run;
//! - two-source concurrent import admission;
//! - Android partial (no retryable counter field, stays ready);
//! - preview retention budget P95.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::import_analysis::ImportAnalysisMode;
use app_services::import_job_coordinator::{
    run_single_import_with_mode, SingleImportMode, SingleImportOutcome, SingleImportRequest,
};
use app_services::timeline_service::{rebuild_timeline_projection, TimelineProjectionPolicy};
use domain::DataSourcePlatform;
use persistence_sqlite::repositories::job_repo::JobRepo;
use persistence_sqlite::DbError;
use transport::dto::{CapabilityKindDto, CapabilityStateDto};

fn sample_path() -> Option<PathBuf> {
    std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(PathBuf::from)
        .or_else(testing::fixtures::local_e01_fixture)
}

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
    let Some(image) = sample_path() else {
        eprintln!("FORENSICS_E01_FIXTURE not set; acceptance harness skipped");
        return;
    };
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
            let outcome = run_single_import_with_mode(
                SingleImportRequest {
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
                },
                SingleImportMode::Detached,
            )
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
                CapabilityStateDto::Pending,
                "timeline must stay pending before projection rebuild"
            );
            assert_eq!(
                matrix_state(&before.sources, &source_id, CapabilityKindDto::Artifacts),
                CapabilityStateDto::NotApplicable,
                "artifacts are notApplicable under MetadataOnly"
            );

            rebuild_timeline_projection(
                case_conn,
                &active.case_root,
                &active.meta.id,
                Some(&source_id),
                TimelineProjectionPolicy::default(),
                &cancel,
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
