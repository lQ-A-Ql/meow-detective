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
    point: RetirementPoint,
    retired: AtomicBool,
    ready: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetirementPoint {
    Merge,
    Finalize,
    Publish,
}

impl RetireAt {
    fn retire(&self) {
        if !self.retired.swap(true, Ordering::AcqRel) {
            source_runtime::invalidate_case(&self.case_id);
        }
    }
}

impl ImportEventSink for RetireAt {
    fn job_progress(&self, _: &str, _: u32, detail: &str) {
        if (self.point == RetirementPoint::Merge && detail == "Merging partitions...")
            || (self.point == RetirementPoint::Finalize && detail == "Finalizing...")
        {
            self.retire();
        }
    }
    fn partition_progress(&self, _: &str, _: &str, _: u32, _: u32, _: u32) {}
    fn data_source_imported(&self, _: &str, _: &DataSourceSummaryDto, _: &str) {
        self.ready.store(true, Ordering::Release);
    }
    fn import_phase_progress(&self, progress: &ImportPhaseProgressDto) {
        if self.point == RetirementPoint::Publish
            && progress.phase == transport::dto::ImportPhaseDto::Finalize
            && progress.state == transport::dto::ImportPhaseStateDto::Completed
        {
            self.retire();
        }
    }
    fn import_partial_result(&self, _: &PartialResultDto) {}
    fn cache_index_status(&self, _: &IndexCacheStatusDto) {}
    fn job_cancellation(&self, _: &JobCancellationDto) {}
}

fn reject_stale_source(point: RetirementPoint, platform: domain::DataSourcePlatform) {
    let temp = tempfile::tempdir().unwrap();
    let image = temp.path().join("source.raw");
    std::fs::write(&image, fixture::image(96)).unwrap();
    let active =
        case_service::create_case(&temp.path().join("cases"), "source fence", None).unwrap();
    let sink = RetireAt {
        case_id: active.meta.id.0.clone(),
        point,
        retired: AtomicBool::new(false),
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
                    projection_policy: Default::default(),
                    event_sink: Some(&sink),
                    cancel_token: &cancel,
                    max_import_workers: Some(2),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: false,
                },
            );
            assert!(
                sink.retired.load(Ordering::Acquire),
                "{point:?} was not reached"
            );
            assert!(
                result.is_err(),
                "stale source reached successful import at {point:?}"
            );
            let (id, state): (String, String) =
                conn.query_row("SELECT id,import_state FROM data_sources", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
            assert_eq!(state, "failed");
            let operation_state: String = conn.query_row(
                "SELECT state FROM storage_operations WHERE operation_id = ?1",
                [format!("source-import:{}", job.0)],
                |r| r.get(0),
            )?;
            assert_eq!(operation_state, "failed");
            let pending = persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRepo::new(conn)
                .list_pending(&active.meta.id.0)?;
            assert!(pending.is_empty());
            assert!(!sink.ready.load(Ordering::Acquire));
            if point == RetirementPoint::Merge {
                let connection = persistence_sqlite::open_existing_source_read_only(
                    &source_db::source_db_path(&active.case_root, &domain::DataSourceId(id)),
                )?;
                let source_id = domain::DataSourceId(connection.query_row(
                    "SELECT id FROM data_sources LIMIT 1",
                    [],
                    |row| row.get(0),
                )?);
                let published_root = source_db::source_db_path(&active.case_root, &source_id)
                    .parent()
                    .expect("source database path has a parent")
                    .join("source.fjall");
                assert!(
                    !published_root.exists(),
                    "invalidated import must not publish a partial Fjall generation"
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn retired_generation_cannot_merge_a_completed_staging_partition() {
    reject_stale_source(RetirementPoint::Merge, domain::DataSourcePlatform::Windows);
}

#[test]
fn invalidation_inside_finalization_event_cannot_publish_ready_or_deadlock() {
    reject_stale_source(
        RetirementPoint::Finalize,
        domain::DataSourcePlatform::Windows,
    );
}

#[test]
fn invalidation_before_linux_capability_queue_cannot_publish_ready() {
    reject_stale_source(RetirementPoint::Publish, domain::DataSourcePlatform::Linux);
}
