#[path = "support/import_ntfs_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use app_services::import_pipeline::{
    execute_import_job_with_counts, ImportEventSink, ImportJobOptions,
};
use app_services::{
    case_service, import_analysis::ImportAnalysisMode, import_precheck, source_db, source_runtime,
};
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use transport::dto::{
    DataSourceSummaryDto, ImportPhaseProgressDto, IndexCacheStatusDto, JobCancellationDto,
    PartialResultDto,
};

struct RetireAt {
    case_id: String,
    progress: u32,
    ready: AtomicBool,
}
impl ImportEventSink for RetireAt {
    fn job_progress(&self, _: &str, progress: u32, _: &str) {
        if progress == self.progress {
            source_runtime::invalidate_case(&self.case_id);
        }
    }
    fn partition_progress(&self, _: &str, _: &str, _: u32, _: u32, _: u32) {}
    fn timeline_updated(&self, _: u64) {}
    fn search_index_progress(&self, _: u32, _: &str) {}
    fn data_source_imported(&self, _: &str, _: &DataSourceSummaryDto, _: &str) {
        self.ready.store(true, Ordering::Release);
    }
    fn import_phase_progress(&self, _: &ImportPhaseProgressDto) {}
    fn import_partial_result(&self, _: &PartialResultDto) {}
    fn cache_index_status(&self, _: &IndexCacheStatusDto) {}
    fn job_cancellation(&self, _: &JobCancellationDto) {}
}

fn reject_stale_source(progress: u32, platform: domain::DataSourcePlatform) {
    let temp = tempfile::tempdir().unwrap();
    let image = temp.path().join("source.raw");
    std::fs::write(&image, fixture::image(96)).unwrap();
    let active =
        case_service::create_case(&temp.path().join("cases"), "source fence", None).unwrap();
    let sink = RetireAt {
        case_id: active.meta.id.0.clone(),
        progress,
        ready: AtomicBool::new(false),
    };
    active
        .with_conn(|conn| {
            let config = import_precheck::prepare_import_source_config_from_path(
                &image.to_string_lossy(),
                platform,
            )
            .unwrap();
            let job = JobRepo::new(conn).create(&active.meta.id.0, "source fence")?;
            let cancel = Arc::new(AtomicBool::new(false));
            let result = execute_import_job_with_counts(
                conn,
                &active.meta.id,
                &active.case_root,
                config,
                &job,
                ImportJobOptions {
                    event_sink: Some(&sink),
                    cancel_token: &cancel,
                    max_import_workers: Some(2),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: false,
                },
            );
            assert!(
                result.is_err(),
                "stale source reached successful import at {progress}"
            );
            let (id, state): (String, String) =
                conn.query_row("SELECT id,import_state FROM data_sources", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
            assert_eq!(state, "failed");
            assert!(!sink.ready.load(Ordering::Acquire));
            if progress == 62 {
                let c = persistence_sqlite::open_existing_source_read_only(
                    &source_db::source_db_path(&active.case_root, &domain::DataSourceId(id)),
                )?;
                assert_eq!(
                    c.query_row("SELECT COUNT(*) FROM file_entries", [], |r| r
                        .get::<_, u64>(0))?,
                    1
                );
                // Attach creates the existing source root before runtime planning.
                // No MFT facts may cross the invalidated merge boundary.
                assert_eq!(
                    c.query_row(
                        "SELECT COUNT(*) FROM file_entries WHERE id LIKE 'mft:%'",
                        [],
                        |r| r.get::<_, u64>(0)
                    )?,
                    0
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn retired_generation_cannot_merge_a_completed_staging_partition() {
    reject_stale_source(62, domain::DataSourcePlatform::Windows);
}

#[test]
fn invalidation_inside_finalization_event_cannot_publish_ready_or_deadlock() {
    reject_stale_source(95, domain::DataSourcePlatform::Windows);
}

#[test]
fn invalidation_at_automatic_linux_analysis_cannot_publish_ready() {
    reject_stale_source(96, domain::DataSourcePlatform::Linux);
}
