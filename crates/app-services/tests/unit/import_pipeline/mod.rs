use crate::import_pipeline::{
    emit::ImportEventSink,
    execute::job_cancellation_dto,
    execute_import_job,
    partition::{format_partition_record_root_name, format_partition_root_name},
    profile::{
        progress::import_phase_progress_from_profile,
        results::{cache_statuses_from_profile, partial_results_from_profile},
    },
    ImportJobOptions,
};
use crate::{case_service, import_analysis, import_precheck, search_service, staging};
use chrono::{DateTime, Utc};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, job_repo::JobRepo};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use transport::dto::{
    DataSourceSummaryDto, ImportPhaseProgressDto, IndexCacheStatusDto, JobCancellationDto,
    PartialResultDto,
};
fn import_config_for_path(path: &std::path::Path) -> import_precheck::ImportSourceConfig {
    import_precheck::prepare_import_source_config_from_path(
        &path.to_string_lossy(),
        domain::DataSourcePlatform::Windows,
    )
    .expect("test import source should be valid")
}

fn single_imported_data_source_id(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
) -> persistence_sqlite::DbResult<domain::DataSourceId> {
    conn.query_row(
        "SELECT id FROM data_sources WHERE case_id = ?1 ORDER BY imported_at DESC LIMIT 1",
        [&case_id.0],
        |row| row.get::<_, String>(0).map(domain::DataSourceId),
    )
    .map_err(Into::into)
}

fn filetime(dt: DateTime<Utc>) -> u64 {
    ((dt.timestamp() + 11_644_473_600) as u64 * 10_000_000)
        + (dt.timestamp_subsec_nanos() as u64 / 100)
}

fn assert_partial_result(
    result: &transport::dto::PartialResultDto,
    kind: transport::dto::PartialResultKindDto,
    scope_id: &str,
    ready_count: u64,
    total_estimate: Option<u64>,
    query_key: &str,
    freshness: transport::dto::ResultFreshnessDto,
) {
    assert_eq!(result.kind, kind);
    assert_eq!(result.scope_id, scope_id);
    assert_eq!(result.ready_count, ready_count);
    assert_eq!(result.total_estimate, total_estimate);
    assert_eq!(result.query_key, query_key);
    assert_eq!(result.freshness, freshness);
}

fn assert_cache_status(
    status: &transport::dto::IndexCacheStatusDto,
    cache_key: &str,
    state: &str,
    indexed_count: u64,
    total_count: Option<u64>,
) {
    assert_eq!(status.cache_key, cache_key);
    assert_eq!(status.state, state);
    assert_eq!(status.indexed_count, indexed_count);
    assert_eq!(status.total_count, total_count);
    assert!(chrono::DateTime::parse_from_rfc3339(&status.updated_at).is_ok());
}

#[derive(Default)]
struct RecordingImportEventSink {
    events: Mutex<Vec<String>>,
}

impl RecordingImportEventSink {
    fn record(&self, event: impl Into<String>) {
        self.events.lock().unwrap().push(event.into());
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

impl ImportEventSink for RecordingImportEventSink {
    fn job_progress(&self, _job_id: &str, progress: u32, detail: &str) {
        self.record(format!("job:{progress}:{detail}"));
    }

    fn partition_progress(
        &self,
        _job_id: &str,
        _current_partition: &str,
        _completed: u32,
        _total: u32,
        _partition_pct: u32,
    ) {
        self.record("partition");
    }

    fn timeline_updated(&self, event_count: u64) {
        self.record(format!("timeline:{event_count}"));
    }

    fn search_index_progress(&self, progress: u32, detail: &str) {
        self.record(format!("search:{progress}:{detail}"));
    }

    fn data_source_imported(
        &self,
        case_id: &str,
        data_source: &DataSourceSummaryDto,
        job_id: &str,
    ) {
        self.record(format!("data-source:{case_id}:{}:{job_id}", data_source.id));
    }

    fn import_phase_progress(&self, progress: &ImportPhaseProgressDto) {
        self.record(format!("phase:{}:{}", progress.job_id, progress.percent));
    }

    fn import_partial_result(&self, result: &PartialResultDto) {
        self.record(format!("partial:{}", result.scope_id));
    }

    fn cache_index_status(&self, status: &IndexCacheStatusDto) {
        self.record(format!("cache:{}", status.cache_key));
    }

    fn job_cancellation(&self, cancellation: &JobCancellationDto) {
        self.record(format!("cancel:{}", cancellation.job_id));
    }
}

#[test]
fn partition_root_names_reject_misleading_filesystem_names() {
    let candidate = crate::datasource_service::ImageFilesystemCandidate {
        partition_index: Some(3),
        partition_name: Some("System Volume Information".to_string()),
        kind: crate::datasource_service::ImageFilesystemKind::Ntfs,
        offset: 2048,
        source: crate::datasource_service::ImageFilesystemSource::GptPartition,
        lvm_identity: None,
    };
    assert_eq!(format_partition_root_name(&candidate), "Partition 3 (NTFS)");

    let root_candidate = crate::datasource_service::ImageFilesystemCandidate {
        partition_name: Some("\\".to_string()),
        ..candidate.clone()
    };
    assert_eq!(
        format_partition_root_name(&root_candidate),
        "Partition 3 (NTFS)"
    );

    let record = crate::datasource_service::PartitionRecord {
        index: 3,
        name: "/".to_string(),
        kind_label: "NTFS".to_string(),
        type_guid: None,
        offset: 2048,
        length: 4096,
        status: crate::datasource_service::PartitionStatus::Supported,
        filesystem: Some(crate::datasource_service::ImageFilesystemKind::Ntfs),
        lvm_identity: None,
    };
    assert_eq!(
        format_partition_record_root_name(&record),
        "Partition 3 (NTFS)"
    );

    let display_record = crate::datasource_service::PartitionRecord {
        name: "Partition 3 (NTFS)".to_string(),
        ..record
    };
    assert_eq!(
        format_partition_record_root_name(&display_record),
        "Partition 3 (NTFS)"
    );
}

#[test]
fn partition_root_names_preserve_meaningful_names() {
    let candidate = crate::datasource_service::ImageFilesystemCandidate {
        partition_index: Some(4),
        partition_name: Some("Evidence Volume".to_string()),
        kind: crate::datasource_service::ImageFilesystemKind::Ntfs,
        offset: 4096,
        source: crate::datasource_service::ImageFilesystemSource::GptPartition,
        lvm_identity: None,
    };

    assert_eq!(
        format_partition_root_name(&candidate),
        "Partition 4 (NTFS) - Evidence Volume"
    );
}

fn prefetch_fixture(exe_name: &str, run_count: u32, last_run: DateTime<Utc>) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0x1Eu32.to_le_bytes());
    data.extend_from_slice(b"SCCA");
    data.extend_from_slice(&0x11u32.to_le_bytes());
    data.extend_from_slice(&0x0000A000u32.to_le_bytes());

    let mut name_buf = vec![0u8; 60];
    for (index, ch) in exe_name.encode_utf16().enumerate() {
        let offset = index * 2;
        if offset + 1 < name_buf.len() {
            name_buf[offset] = (ch & 0xFF) as u8;
            name_buf[offset + 1] = (ch >> 8) as u8;
        }
    }
    data.extend_from_slice(&name_buf);
    data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    let mut file_info = vec![0u8; 212];
    file_info[0..4].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[8..12].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[16..20].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[24..28].copy_from_slice(&0x128u32.to_le_bytes());
    file_info[44..52].copy_from_slice(&filetime(last_run).to_le_bytes());
    file_info[116..120].copy_from_slice(&run_count.to_le_bytes());
    file_info[120..124].copy_from_slice(&1u32.to_le_bytes());
    file_info[124..128].copy_from_slice(&3u32.to_le_bytes());
    file_info[128..132].copy_from_slice(&0x128u32.to_le_bytes());
    data.extend_from_slice(&file_info);

    data.resize(4096, 0);
    data
}

#[test]
fn import_profile_progress_maps_enumeration_metrics_to_typed_dto() {
    let dto = import_phase_progress_from_profile(
        &domain::JobId("job-1".to_string()),
        &domain::CaseId("case-1".to_string()),
        Some(&domain::DataSourceId("ds-1".to_string())),
        60,
        "Enumeration complete: phase=enumeration elapsedMs=125 rows=12 rowsPerSec=96 dataMb=3 mbPerSec=24 workers=4 rssMb=512",
        false,
    );

    assert_eq!(dto.job_id, "job-1");
    assert_eq!(dto.case_id, "case-1");
    assert_eq!(dto.data_source_id.as_deref(), Some("ds-1"));
    assert_eq!(dto.phase, transport::dto::ImportPhaseDto::Enumerate);
    assert_eq!(dto.state, transport::dto::ImportPhaseStateDto::Completed);
    assert_eq!(dto.percent, 60);
    assert_eq!(dto.metrics.elapsed_ms, 125);
    assert_eq!(dto.metrics.rss_mb, 512);
    assert_eq!(dto.metrics.workers, 4);
    assert_eq!(dto.metrics.rows_processed, 12);
    assert_eq!(dto.metrics.rows_per_sec, Some(96.0));
    assert_eq!(dto.metrics.bytes_processed, 3 * 1024 * 1024);
    assert_eq!(dto.metrics.mb_per_sec, Some(24.0));
    assert!(dto.partial_results.is_empty());
    assert!(dto.cancellable);
    assert!(!dto.cancel_requested);
}

#[test]
fn enum_merge_progress_exposes_ready_file_results() {
    let dto = import_phase_progress_from_profile(
        &domain::JobId("job-files".to_string()),
        &domain::CaseId("case-files".to_string()),
        Some(&domain::DataSourceId("ds-files".to_string())),
        70,
        "File catalog ready: phase=enum-merge rows=9 files=6 dirs=3 warnings=0 rssMb=128",
        false,
    );

    assert_eq!(dto.phase, transport::dto::ImportPhaseDto::MergeEnumeration);
    assert_eq!(dto.state, transport::dto::ImportPhaseStateDto::Completed);
    assert_eq!(dto.partial_results.len(), 2);
    assert_partial_result(
        &dto.partial_results[0],
        transport::dto::PartialResultKindDto::FileRows,
        "ds-files",
        9,
        Some(9),
        "files:rows:ds-files",
        transport::dto::ResultFreshnessDto::Ready,
    );
    assert_partial_result(
        &dto.partial_results[1],
        transport::dto::PartialResultKindDto::FileTree,
        "ds-files",
        9,
        Some(9),
        "files:tree:ds-files",
        transport::dto::ResultFreshnessDto::Ready,
    );
}

#[test]
fn analysis_progress_exposes_partial_search_index_result() {
    let dto = import_phase_progress_from_profile(
        &domain::JobId("job-search".to_string()),
        &domain::CaseId("case-search".to_string()),
        Some(&domain::DataSourceId("ds-search".to_string())),
        75,
        "Analysis heartbeat: phase=analysis scheduling=running memory=ok rssMb=256 workerBudget=4 queuedTasks=2 pendingTasks=5 processed=5/10 indexed=4 activeWorkers=2",
        false,
    );

    assert_eq!(dto.partial_results.len(), 1);
    assert_partial_result(
        &dto.partial_results[0],
        transport::dto::PartialResultKindDto::SearchIndex,
        "ds-search",
        4,
        Some(10),
        "search:index:ds-search",
        transport::dto::ResultFreshnessDto::Partial,
    );
}

#[test]
fn scheduling_profiles_expose_worker_budget_and_deferred_states() {
    let queued = import_phase_progress_from_profile(
        &domain::JobId("job-schedule".to_string()),
        &domain::CaseId("case-schedule".to_string()),
        Some(&domain::DataSourceId("ds-schedule".to_string())),
        72,
        "Analysis staging: phase=analysis-start scheduling=queued mode=budgetedContent workers=3 workerBudget=3 activeWorkers=0 queuedTasks=0 pendingTasks=42 queueBound=768 content=enabled text=enabled contentDeferred=false textDeferred=false rssMb=128",
        false,
    );

    assert_eq!(queued.phase, transport::dto::ImportPhaseDto::Analyze);
    assert_eq!(queued.state, transport::dto::ImportPhaseStateDto::Running);
    assert_eq!(queued.metrics.workers, 3);
    assert_eq!(queued.metrics.rows_total, Some(42));
    assert!(queued.detail.contains("scheduling=queued"));
    assert!(queued.detail.contains("workerBudget=3"));
    assert!(queued.detail.contains("pendingTasks=42"));

    let deferred = import_phase_progress_from_profile(
        &domain::JobId("job-deferred".to_string()),
        &domain::CaseId("case-deferred".to_string()),
        Some(&domain::DataSourceId("ds-deferred".to_string())),
        84,
        "Post-import skipped: phase=post-import-skip scheduling=deferred workerBudget=2 activeWorkers=0 queuedTasks=0 pendingTasks=0 timeline=deferred content=disabled text=disabled contentDeferred=true textDeferred=true",
        false,
    );

    assert_eq!(deferred.phase, transport::dto::ImportPhaseDto::Finalize);
    assert_eq!(deferred.state, transport::dto::ImportPhaseStateDto::Skipped);
    assert_eq!(deferred.metrics.workers, 2);
    assert!(deferred.detail.contains("scheduling=deferred"));
    assert!(deferred.detail.contains("contentDeferred=true"));
    assert!(deferred.detail.contains("textDeferred=true"));
    assert_eq!(deferred.partial_results.len(), 3);
    assert!(deferred
        .partial_results
        .iter()
        .all(|result| result.freshness == transport::dto::ResultFreshnessDto::Deferred));
}

#[test]
fn scheduling_profiles_expose_throttled_and_draining_states() {
    let throttled = import_phase_progress_from_profile(
        &domain::JobId("job-throttle".to_string()),
        &domain::CaseId("case-throttle".to_string()),
        Some(&domain::DataSourceId("ds-throttle".to_string())),
        75,
        "Analysis heartbeat: phase=analysis scheduling=throttled memory=soft-limit rssMb=4096 softLimitMb=4096 hardLimitMb=6144 workerBudget=4 queuedTasks=100 pendingTasks=25 processed=75/100 indexed=20 activeWorkers=4",
        false,
    );

    assert_eq!(throttled.phase, transport::dto::ImportPhaseDto::Analyze);
    assert_eq!(
        throttled.state,
        transport::dto::ImportPhaseStateDto::Running
    );
    assert_eq!(throttled.metrics.workers, 4);
    assert_eq!(throttled.metrics.rows_processed, 75);
    assert_eq!(throttled.metrics.rows_total, Some(100));
    assert!(throttled.detail.contains("scheduling=throttled"));
    assert!(throttled.detail.contains("memory=soft-limit"));

    let draining = import_phase_progress_from_profile(
        &domain::JobId("job-drain".to_string()),
        &domain::CaseId("case-drain".to_string()),
        Some(&domain::DataSourceId("ds-drain".to_string())),
        75,
        "Analysis memory hard limit exceeded: phase=analysis scheduling=draining rssMb=6144 hardLimitMb=6144 workerBudget=4 queuedTasks=100 pendingTasks=25 processed=75 activeWorkers=4",
        true,
    );

    assert_eq!(draining.phase, transport::dto::ImportPhaseDto::Analyze);
    assert_eq!(
        draining.state,
        transport::dto::ImportPhaseStateDto::Cancelling
    );
    assert_eq!(draining.metrics.workers, 4);
    assert!(draining.cancel_requested);
    assert!(draining.detail.contains("scheduling=draining"));
}

#[test]
fn post_import_profiles_expose_deferred_ready_stale_and_invalidated_results() {
    let skipped = partial_results_from_profile(
        Some(&domain::DataSourceId("ds-deferred".to_string())),
        "Post-import skipped: phase=post-import-skip timeline=deferred content=disabled text=disabled",
    );
    assert_eq!(skipped.len(), 3);
    assert_partial_result(
        &skipped[0],
        transport::dto::PartialResultKindDto::TimelineEvents,
        "ds-deferred",
        0,
        None,
        "timeline:events:ds-deferred",
        transport::dto::ResultFreshnessDto::Deferred,
    );
    assert_partial_result(
        &skipped[1],
        transport::dto::PartialResultKindDto::ArtifactFamily,
        "ds-deferred",
        0,
        None,
        "artifacts:family:ds-deferred",
        transport::dto::ResultFreshnessDto::Deferred,
    );
    assert_partial_result(
        &skipped[2],
        transport::dto::PartialResultKindDto::SearchIndex,
        "ds-deferred",
        0,
        None,
        "search:index:ds-deferred",
        transport::dto::ResultFreshnessDto::Deferred,
    );

    let ready = partial_results_from_profile(
        Some(&domain::DataSourceId("ds-ready".to_string())),
        "Post-import complete: phase=post-import elapsedMs=42 timeline=8 artifacts=2 indexed=5 rssMb=128",
    );
    assert_partial_result(
        &ready[0],
        transport::dto::PartialResultKindDto::TimelineEvents,
        "ds-ready",
        8,
        Some(8),
        "timeline:events:ds-ready",
        transport::dto::ResultFreshnessDto::Ready,
    );
    assert_partial_result(
        &ready[1],
        transport::dto::PartialResultKindDto::ArtifactFamily,
        "ds-ready",
        2,
        Some(2),
        "artifacts:family:ds-ready",
        transport::dto::ResultFreshnessDto::Ready,
    );
    assert_partial_result(
        &ready[2],
        transport::dto::PartialResultKindDto::SearchIndex,
        "ds-ready",
        5,
        Some(5),
        "search:index:ds-ready",
        transport::dto::ResultFreshnessDto::Ready,
    );

    let stale = partial_results_from_profile(
        Some(&domain::DataSourceId("ds-stale".to_string())),
        "Analysis staging already merged; skipping analysis resume.",
    );
    assert_partial_result(
        &stale[2],
        transport::dto::PartialResultKindDto::SearchIndex,
        "ds-stale",
        0,
        None,
        "search:index:ds-stale",
        transport::dto::ResultFreshnessDto::Stale,
    );

    let invalidated = partial_results_from_profile(
        Some(&domain::DataSourceId("ds-invalidated".to_string())),
        "Analysis staging layout changed; reinitializing unfinished worker DBs: previousWorkers=[0] currentWorkers=[0, 1]",
    );
    assert_partial_result(
        &invalidated[2],
        transport::dto::PartialResultKindDto::SearchIndex,
        "ds-invalidated",
        0,
        None,
        "search:index:ds-invalidated",
        transport::dto::ResultFreshnessDto::Invalidated,
    );
}

#[test]
fn cache_status_profiles_expose_warming_ready_deferred_reused_stale_and_invalidated_states() {
    let warming = cache_statuses_from_profile(
        Some(&domain::DataSourceId("ds-warming".to_string())),
        "Analysis heartbeat: phase=analysis scheduling=running memory=ok processed=4/10 indexed=3 activeWorkers=2",
    );
    assert_eq!(warming.len(), 3);
    assert_cache_status(
        &warming[2],
        "search:index:ds-warming",
        "warming",
        3,
        Some(10),
    );

    let ready = cache_statuses_from_profile(
        Some(&domain::DataSourceId("ds-ready".to_string())),
        "Post-import complete: phase=post-import elapsedMs=42 timeline=8 artifacts=2 indexed=5 rssMb=128",
    );
    assert_cache_status(&ready[0], "timeline:events:ds-ready", "ready", 8, Some(8));
    assert_cache_status(&ready[1], "artifacts:family:ds-ready", "ready", 2, Some(2));
    assert_cache_status(&ready[2], "search:index:ds-ready", "ready", 5, Some(5));

    let deferred = cache_statuses_from_profile(
        Some(&domain::DataSourceId("ds-deferred".to_string())),
        "Post-import skipped: phase=post-import-skip scheduling=deferred timeline=deferred content=disabled text=disabled",
    );
    assert!(deferred
        .iter()
        .all(|status| status.state == "deferred" && status.total_count.is_none()));

    let reused = cache_statuses_from_profile(
        Some(&domain::DataSourceId("ds-reused".to_string())),
        "Analysis staging already merged; skipping analysis resume.",
    );
    assert!(reused.iter().all(|status| status.state == "reused"));

    let stale = cache_statuses_from_profile(
        Some(&domain::DataSourceId("ds-stale".to_string())),
        "Merging analysis staging DBs...",
    );
    assert!(stale.iter().all(|status| status.state == "stale"));

    let invalidated = cache_statuses_from_profile(
        Some(&domain::DataSourceId("ds-invalidated".to_string())),
        "Analysis staging layout changed; reinitializing unfinished worker DBs: previousWorkers=[0] currentWorkers=[0, 1]",
    );
    assert!(invalidated
        .iter()
        .all(|status| status.state == "invalidated"));
}

#[test]
fn import_profile_progress_maps_analysis_and_cancel_state() {
    let dto = import_phase_progress_from_profile(
        &domain::JobId("job-2".to_string()),
        &domain::CaseId("case-2".to_string()),
        None,
        75,
        "Analysis heartbeat: phase=analysis memory=ok rssMb=256 softLimitMb=1536 hardLimitMb=3072 queuedTasks=2 processed=5/10 indexed=4 activeWorkers=2",
        true,
    );

    assert_eq!(dto.data_source_id, None);
    assert_eq!(dto.phase, transport::dto::ImportPhaseDto::Analyze);
    assert_eq!(dto.state, transport::dto::ImportPhaseStateDto::Cancelling);
    assert_eq!(dto.metrics.rss_mb, 256);
    assert_eq!(dto.metrics.workers, 2);
    assert_eq!(dto.metrics.rows_processed, 5);
    assert_eq!(dto.metrics.rows_total, Some(10));
    assert!(dto.cancel_requested);
}

#[test]
fn import_profile_progress_serializes_as_phase_progress_payload() {
    let dto = import_phase_progress_from_profile(
        &domain::JobId("job-3".to_string()),
        &domain::CaseId("case-3".to_string()),
        Some(&domain::DataSourceId("ds-3".to_string())),
        99,
        "Import profile complete: phase=total elapsedMs=1000 rssMb=128",
        false,
    );
    let value = serde_json::to_value(dto).expect("serialize typed import progress");

    assert_eq!(value["phase"], "finalize");
    assert_eq!(value["state"], "completed");
    assert_eq!(value["percent"], 99);
    assert_eq!(value["cancellable"], false);
    assert_eq!(value["metrics"]["elapsedMs"], 1000);
    assert!(value.get("progress").is_none());
    assert!(value.get("job_id").is_none());
}

#[test]
fn job_cancellation_dto_maps_requested_and_draining_states() {
    let requested = job_cancellation_dto(
        "job-cancel-1",
        transport::dto::CancellationStateDto::Requested,
        false,
        "Cancel requested by user",
    );
    assert_eq!(requested.job_id, "job-cancel-1");
    assert_eq!(
        requested.state,
        transport::dto::CancellationStateDto::Requested
    );
    assert!(!requested.safe_to_close);
    assert!(requested.requested_at.is_some());
    assert!(requested.acknowledged_at.is_none());

    let draining = job_cancellation_dto(
        "job-cancel-1",
        transport::dto::CancellationStateDto::Draining,
        false,
        "Cancellation acknowledged; draining workers",
    );
    assert_eq!(
        draining.state,
        transport::dto::CancellationStateDto::Draining
    );
    assert!(!draining.safe_to_close);
    assert!(draining.requested_at.is_some());
    assert!(draining.acknowledged_at.is_some());
}

#[test]
fn cancellation_after_attach_marks_job_cancelling_without_failure() {
    let tmp = TempDir::new().unwrap();
    let evidence_dir = tmp.path().join("evidence-cancel");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(evidence_dir.join("notes.txt"), "cancel seam").unwrap();

    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "cancel-after-attach",
        Some("tester"),
    )
    .unwrap();
    let cancel = Arc::new(AtomicBool::new(true));

    active
        .with_conn(|conn| {
            let job_id = JobRepo::new(conn)
                .create(&active.meta.id.0, "Import cancel")
                .unwrap();
            let result = execute_import_job(
                conn,
                &active.meta.id,
                &active.case_root,
                import_config_for_path(&evidence_dir),
                &job_id,
                ImportJobOptions {
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: None,
                    max_analysis_workers: None,
                    analysis_mode: import_analysis::ImportAnalysisMode::MetadataOnly,
                },
            );

            assert!(matches!(result, Err(ref error) if error.message.contains("cancelled")));
            let job = JobRepo::new(conn)
                .list_recent(10)
                .unwrap()
                .into_iter()
                .find(|job| job.id.0 == job_id.0)
                .unwrap();
            assert_eq!(job.status, "cancelling");
            assert_eq!(job.detail, "Cancellation acknowledged after attach");

            Ok(())
        })
        .unwrap();
}

#[test]
fn logical_import_post_pipeline_indexes_marker_and_extracts_artifact() {
    let tmp = TempDir::new().unwrap();
    let evidence_dir = tmp.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();

    let marker = "fw_marker_8f15d3f2c9e64b51";
    std::fs::write(
        evidence_dir.join("notes.txt"),
        format!("Forensics import marker: {marker}"),
    )
    .unwrap();
    std::fs::write(
        evidence_dir.join("CMD.EXE-DEADBEEF.pf"),
        prefetch_fixture("CMD.EXE", 3, Utc::now()),
    )
    .unwrap();

    let active =
        case_service::create_case(&tmp.path().join("cases"), "post-import", Some("tester"))
            .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));

    active
        .with_conn(|conn| {
            let job_id = JobRepo::new(conn).create(&active.meta.id.0, "Import test")?;
            let message = execute_import_job(
                conn,
                &active.meta.id,
                &active.case_root,
                import_config_for_path(&evidence_dir),
                &job_id,
                ImportJobOptions {
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: None,
                    max_analysis_workers: None,
                    analysis_mode: import_analysis::ImportAnalysisMode::MetadataOnly,
                },
            )
            .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

            assert!(message.contains("Index:"));

            let data_sources: i64 = conn.query_row(
                "SELECT COUNT(*) FROM data_sources WHERE case_id = ?1 AND kind = 'logical_directory'",
                [&active.meta.id.0],
                |row| row.get(0),
            )?;
            assert_eq!(data_sources, 1);
            let data_source_id = single_imported_data_source_id(conn, &active.meta.id)?;
            let source_conn =
                crate::source_db::open_source_db(&active.case_root, &data_source_id)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let file_entries: i64 = conn.query_row(
                "SELECT COUNT(*) FROM file_entries",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(file_entries, 0, "app.db must not own source file entries");
            let source_file_entries: i64 = source_conn.query_row(
                "SELECT COUNT(*) FROM file_entries WHERE entry_type = 'file'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(source_file_entries, 2);

            let timeline_events: i64 = conn.query_row(
                "SELECT COUNT(*) FROM timeline_events",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(timeline_events, 0, "app.db must not own source timeline events");
            let app_graph_nodes: i64 =
                conn.query_row("SELECT COUNT(*) FROM graph_nodes", [], |row| row.get(0))?;
            assert_eq!(app_graph_nodes, 0, "app.db must not own source-local graph");
            let source_timeline_events: i64 = source_conn.query_row(
                "SELECT COUNT(*) FROM timeline_events",
                [],
                |row| row.get(0),
            )?;
            assert!(source_timeline_events > 0);
            let source_graph_nodes: i64 =
                source_conn.query_row("SELECT COUNT(*) FROM graph_nodes", [], |row| row.get(0))?;
            assert!(source_graph_nodes > 0, "source.db should own source-local graph");

            let index_dir = crate::source_db::source_index_dir(&active.case_root, &data_source_id);
            let results = search_service::search_files_real(&index_dir, marker, 0, 10)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert_eq!(results.total, 1);
            assert!(results.items[0].path.ends_with("notes.txt"));

            let artifact_repo = ArtifactRepo::new(&source_conn);
            assert!(artifact_repo.count()? > 0);
            let families = artifact_repo.families()?;
            assert!(families.iter().any(|family| family == "Prefetch"));

            let metrics = case_service::get_case_metrics_for_case(
                conn,
                &active.case_root,
                &active.meta.id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(metrics.data_source_count, 1);
            assert!(metrics.indexed_file_count > 0);
            assert!(metrics.timeline_event_count > 0);
            assert!(metrics.artifact_count > 0);

            Ok(())
        })
        .unwrap();
}

#[test]
fn logical_import_reports_progress_through_tauri_free_sink() {
    let tmp = TempDir::new().unwrap();
    let evidence_dir = tmp.path().join("evidence-sink");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(evidence_dir.join("notes.txt"), "sink marker").unwrap();

    let active =
        case_service::create_case(&tmp.path().join("cases"), "sink-import", Some("tester"))
            .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let event_sink = RecordingImportEventSink::default();

    active
        .with_conn(|conn| {
            let job_id = JobRepo::new(conn).create(&active.meta.id.0, "Import sink")?;
            execute_import_job(
                conn,
                &active.meta.id,
                &active.case_root,
                import_config_for_path(&evidence_dir),
                &job_id,
                ImportJobOptions {
                    event_sink: Some(&event_sink),
                    cancel_token: &cancel,
                    max_import_workers: None,
                    max_analysis_workers: Some(1),
                    analysis_mode: import_analysis::ImportAnalysisMode::MetadataOnly,
                },
            )
            .map_err(|err| persistence_sqlite::DbError::System(err.message))?;
            Ok(())
        })
        .unwrap();

    let events = event_sink.events();
    assert!(
        events
            .iter()
            .any(|event| event.contains("Attaching data source")),
        "sink should receive job progress events: {events:?}"
    );
    assert!(
        events.iter().any(|event| event.starts_with("phase:")),
        "sink should receive typed phase progress events: {events:?}"
    );
    assert!(
        events.iter().any(|event| event.starts_with("timeline:")),
        "sink should receive finalize timeline events: {events:?}"
    );
    assert!(
        events.iter().any(|event| event.starts_with("data-source:")),
        "sink should receive imported data-source events: {events:?}"
    );
}

#[test]
fn image_backed_metadata_only_post_import_defers_timeline_until_query() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "raw-lazy-timeline",
        Some("tester"),
    )
    .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));

    active
        .with_conn(|conn| {
            let _job_id = JobRepo::new(conn).create(&active.meta.id.0, "Raw import seam")?;
            let data_source_id = domain::DataSourceId("raw-ds-1".to_string());
            conn.execute(
                "INSERT INTO data_sources (id, case_id, name, kind, source_path)
                 VALUES (?1, ?2, 'sample.raw', 'raw', 'C:/evidence/sample.raw')",
                rusqlite::params![data_source_id.0, active.meta.id.0],
            )?;
            conn.execute(
                "INSERT INTO file_entries
                 (id, data_source_id, path, name, entry_type, size, ext, deleted,
                  created_at, modified_at, accessed_at, changed_at)
                 VALUES
                 ('raw-file-1', ?1, '/Windows/System32/config/SYSTEM', 'SYSTEM', 'file', 4096,
                  NULL, 0, '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z',
                  '2026-01-03T00:00:00Z', '2026-01-04T00:00:00Z')",
                [&data_source_id.0],
            )?;

            let index_dir = active.case_root.join("indexes").join("tantivy");
            let (message, counts) = import_analysis::run_post_import_pipeline_with_counts(
                import_analysis::PostImportPipelineOptions {
                    case_root: active.case_root.clone(),
                    db_path: active.case_root.join("app.db"),
                    case_id: active.meta.id.0.clone(),
                    data_source_id: data_source_id.clone(),
                    platform: domain::DataSourcePlatform::Windows,
                    index_dir: index_dir.clone(),
                    max_analysis_workers: Some(1),
                    cancel_token: Arc::clone(&cancel),
                    enable_timeline_projection: false,
                    enable_content_extraction: false,
                    enable_text_indexing: false,
                    analysis_mode: import_analysis::ImportAnalysisMode::MetadataOnly,
                    tier_state: Arc::new(std::sync::Mutex::new(
                        import_analysis::tier::TierStateMachine::new(),
                    )),
                },
                None,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

            assert_eq!(
                message,
                "Timeline: deferred until Timeline page. Artifacts: 0. Index: 0 indexed"
            );
            assert!(!counts.is_partial());
            let before_query: i64 =
                conn.query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))?;
            assert_eq!(before_query, 0);

            let page = crate::timeline_service::query_timeline(conn, 0, 10)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert_eq!(page.total, 4);
            assert_eq!(page.items.len(), 4);
            assert!(page
                .items
                .iter()
                .any(|event| event.id == "macb:raw-file-1:FILE_CREATED"));

            let second = crate::timeline_service::ensure_macb_timeline_projected(conn)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(second.already_projected);
            assert_eq!(second.inserted_count, 0);

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn e01_full_import() {
    let e01_path = std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_E01_FIXTURE to run real E01 import profile test");
    assert!(
        e01_path.exists(),
        "FORENSICS_E01_FIXTURE does not exist: {}",
        e01_path.display()
    );
    let tmp = tempfile::TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "regression", Some("tester")).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    eprintln!("=== E01 Full Import Regression Test ===");
    eprintln!("Source: {}", e01_path.display());
    eprintln!("Case ID: {}", active.meta.id.0);

    let t_total = std::time::Instant::now();

    active
        .with_conn(|conn| {
            let job_id = JobRepo::new(conn).create(&active.meta.id.0, "Import regression")?;

            eprintln!("\n[1/5] Starting import...");
            let t_import = std::time::Instant::now();
            let result = execute_import_job(
                conn,
                &active.meta.id,
                &active.case_root,
                import_config_for_path(&e01_path),
                &job_id,
                ImportJobOptions {
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: None,
                    max_analysis_workers: None,
                    analysis_mode: import_analysis::ImportAnalysisMode::MetadataOnly,
                },
            );
            match &result {
                Ok(msg) => eprintln!(
                    "  Import completed in {:.1}s: {}",
                    t_import.elapsed().as_secs_f64(),
                    msg
                ),
                Err(e) => {
                    eprintln!("  Import FAILED: {:?}", e);
                    return Err(persistence_sqlite::DbError::System(format!(
                        "Import failed: {:?}",
                        e
                    )));
                }
            }

            let data_source_id = single_imported_data_source_id(conn, &active.meta.id)?;
            let source_conn =
                crate::source_db::open_source_db(&active.case_root, &data_source_id)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            eprintln!("\n[2/5] Verifying file entries...");
            let file_count: i64 =
                source_conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
            eprintln!("  File entries: {}", file_count);
            assert!(file_count > 0, "Expected file entries, got 0");
            let root_system32: i64 = source_conn.query_row(
                "SELECT COUNT(*) FROM file_entries
                 WHERE parent_id = 'mft:3:5' AND name = 'System32' COLLATE NOCASE",
                [],
                |row| row.get(0),
            )?;
            let root_windows: i64 = source_conn.query_row(
                "SELECT COUNT(*) FROM file_entries
                 WHERE parent_id = 'mft:3:5'
                   AND entry_type = 'directory' COLLATE NOCASE
                   AND name = 'Windows' COLLATE NOCASE",
                [],
                |row| row.get(0),
            )?;
            let system_hives: i64 = source_conn.query_row(
                "SELECT COUNT(*) FROM file_entries
                 WHERE LOWER(REPLACE(path, '\\', '/')) IN (
                   'windows/system32/config/system',
                   'windows/system32/config/software',
                   'windows/system32/winevt/logs/system.evtx'
                 )",
                [],
                |row| row.get(0),
            )?;
            eprintln!(
                "  NTFS shape: root Windows={}, root System32={}, key hives/logs={}",
                root_windows, root_system32, system_hives
            );
            assert_eq!(
                root_system32, 0,
                "System32 must not be flattened under NTFS root"
            );
            assert!(
                root_windows > 0,
                "Expected Windows directory under NTFS root"
            );
            assert!(
                system_hives >= 2,
                "Expected Windows registry/event-log paths after NTFS import"
            );

            eprintln!("\n[3/5] Verifying timeline lazy projection...");
            let tl_count_before: i64 = source_conn.query_row(
                "SELECT COUNT(*) FROM timeline_events",
                [],
                |row| row.get(0),
            )?;
            eprintln!("  Timeline events before page query: {}", tl_count_before);
            assert_eq!(
                tl_count_before, 0,
                "metadata-only import should defer MACB timeline projection"
            );
            let timeline_page = crate::timeline_service::query_timeline(&source_conn, 0, 10)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let tl_count = timeline_page.total as i64;
            eprintln!("  Timeline events after lazy query: {}", tl_count);
            assert!(tl_count > 0, "Expected lazy timeline events, got 0");

            eprintln!("\n[4/6] Verifying system information analysis...");
            let system_info =
                crate::analysis_service::extract_system_info_for_case(
                    conn,
                    |file_id, max_bytes| {
                        crate::file_service::read_file_header_by_id(
                            conn, file_id, max_bytes,
                        )
                    },
                );
            eprintln!(
                "  System info: status={:?}, computer={:?}, os={:?}, build={:?}, timezone={:?}, bootRecords={}, warnings={}",
                system_info.status,
                system_info.computer_name,
                system_info.os_version,
                system_info.build_number,
                system_info.timezone,
                system_info.boot_history.len(),
                system_info.warnings.len()
            );
            for warning in &system_info.warnings {
                eprintln!("  System info warning: {warning}");
            }
            if system_info.status == transport::dto::AnalysisParseStatusDto::NotParsed
                || system_info.status == transport::dto::AnalysisParseStatusDto::Unavailable
            {
                eprintln!(
                    "  System info not parsed for this sample; NTFS import is valid but artifact parsers need follow-up."
                );
            } else if system_info.status == transport::dto::AnalysisParseStatusDto::Partial {
                eprintln!(
                    "  System info partially parsed; remaining parser warnings are listed above."
                );
            }

            eprintln!("\n[5/7] Verifying evidence semantic classification...");
            let evidence_summary = crate::analysis_service::get_evidence_classification_summary(
                conn,
                domain::DataSourcePlatform::Windows,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            eprintln!(
                "  Evidence summary: status={:?}, categories={}, candidates={}, artifacts={}, totalSizeMb={}",
                evidence_summary.status,
                evidence_summary.totals.category_count,
                evidence_summary.totals.candidate_file_count,
                evidence_summary.totals.artifact_count,
                evidence_summary.totals.total_size / (1024 * 1024)
            );
            for category in &evidence_summary.categories {
                if category.file_count > 0 || category.artifact_count > 0 {
                    eprintln!(
                        "    {} status={:?} files={} artifacts={} sources={}",
                        category.category,
                        category.status,
                        category.file_count,
                        category.artifact_count,
                        category.sources.len()
                    );
                }
            }
            let evidence_category = |name: &str| {
                evidence_summary
                    .categories
                    .iter()
                    .find(|category| category.category == name)
                    .expect("evidence category should exist")
            };
            assert!(
                matches!(
                    evidence_category("SystemInformation").status,
                    transport::dto::AnalysisParseStatusDto::CandidateFound
                        | transport::dto::AnalysisParseStatusDto::Parsed
                        | transport::dto::AnalysisParseStatusDto::Partial
                ),
                "SystemInformation should not be a fake empty category"
            );
            assert!(
                matches!(
                    evidence_category("EventLogs").status,
                    transport::dto::AnalysisParseStatusDto::CandidateFound
                        | transport::dto::AnalysisParseStatusDto::Parsed
                        | transport::dto::AnalysisParseStatusDto::Partial
                ),
                "EventLogs should not be a fake empty category"
            );
            assert!(
                evidence_summary.totals.candidate_file_count > 0,
                "Expected semantic evidence candidates after NTFS import"
            );

            eprintln!("\n[6/7] Verifying optional post-import content outputs...");
            let artifact_count: i64 =
                conn.query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))?;
            eprintln!("  Artifacts: {}", artifact_count);
            let index_rows: i64 = staging::analysis_staging_db_path(
                &active.case_root,
                &{
                    let ds_id: String = conn.query_row(
                        "SELECT id FROM data_sources ORDER BY imported_at DESC LIMIT 1",
                        [],
                        |row| row.get(0),
                    )?;
                    ds_id
                },
                0,
            )
            .exists() as i64;
            eprintln!("  Analysis staging exists: {}", index_rows > 0);

            eprintln!("\n[7/7] Verifying job status...");
            let job = JobRepo::new(conn)
                .list_recent(10)
                .unwrap()
                .into_iter()
                .find(|j| j.id.0 == job_id.0)
                .unwrap();
            eprintln!("  Job status: {}", job.status);
            assert_eq!(job.status, "running");

            let total_time = t_total.elapsed().as_secs_f64();
            eprintln!("\n=== Regression Test PASSED ===");
            eprintln!("Total time: {:.1}s", total_time);
            eprintln!(
                "Files: {}, Timeline: {}, Artifacts: {}, SystemInfo={:?}",
                file_count, tl_count, artifact_count, system_info.status
            );

            Ok(())
        })
        .unwrap();
}
