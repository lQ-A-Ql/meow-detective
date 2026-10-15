use super::*;
use crate::datasource_service::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemSource,
};
use crate::import_pipeline::{build_partition_work, ImportProfiler};
use crate::parallel_enum::partition_work::enumerate_single_partition_profiled;
use domain::DataSourceKind;
use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
};

#[path = "../../support/import_ntfs_fixture.rs"]
mod fixture;

fn candidate() -> ImageFilesystemCandidate {
    ImageFilesystemCandidate {
        partition_index: Some(1),
        partition_name: Some("volume".into()),
        kind: ImageFilesystemKind::Ntfs,
        offset: 0,
        length: None,
        source: ImageFilesystemSource::DirectVolume,
        lvm_identity: None,
    }
}

fn deferred(path: &std::path::Path, kind: DataSourceKind) -> PartitionWork {
    build_partition_work(path, &kind, 1, "NTFS", "ntfs", &[candidate()]).unwrap()
}

#[test]
fn lazy_planning_does_not_open_the_evidence_and_preserves_the_candidate() {
    let mut plan = candidate();
    plan.source = ImageFilesystemSource::GptPartition;
    plan.offset = 4096;
    plan.length = Some(1234567);
    let work = build_partition_work(
        std::path::Path::new("missing-synthetic.E01"),
        &DataSourceKind::E01,
        1,
        "NTFS",
        "ntfs",
        &[plan.clone()],
    )
    .unwrap();
    match work.strategy {
        PartitionEnumStrategy::NtfsDeferred(deferred) => assert_eq!(deferred.candidate, plan),
        PartitionEnumStrategy::Generic(_) => panic!("expected a deferred reader"),
    }
}

#[test]
fn lazy_eligibility_excludes_other_sources_and_non_direct_views() {
    let plan = candidate();
    for kind in [
        DataSourceKind::LocalDisk,
        DataSourceKind::AndroidSparse,
        DataSourceKind::LogicalDirectory,
        DataSourceKind::LogicalArchive,
        DataSourceKind::CephRbd,
        DataSourceKind::CephFs,
    ] {
        assert!(!NtfsPartitionPlan::eligible(&kind, &plan));
    }
    let mut lvm = plan.clone();
    lvm.source = ImageFilesystemSource::LvmLogicalVolume;
    assert!(!NtfsPartitionPlan::eligible(&DataSourceKind::Raw, &lvm));
    let mut other_fs = plan;
    other_fs.kind = ImageFilesystemKind::Fat;
    assert!(!NtfsPartitionPlan::eligible(
        &DataSourceKind::Raw,
        &other_fs
    ));
}

#[test]
fn completed_staging_reuses_results_without_opening_a_missing_source() {
    let temp = tempfile::TempDir::new().unwrap();
    let conn = staging::open_partition_staging(temp.path(), "done-source", 1).unwrap();
    staging::set_staging_meta(&conn, "status", "done").unwrap();
    drop(conn);
    let profiler = ImportProfiler::default();
    let result = enumerate_single_partition_profiled(
        temp.path(),
        "done-source",
        deferred(&temp.path().join("missing.raw"), DataSourceKind::Raw),
        &AtomicBool::new(false),
        None,
        Some(profiler.clone()),
    );
    assert!(result.error.is_none());
    assert!(profiler.report().samples.is_empty());
}

#[test]
fn prefixed_volume_keeps_its_read_offset_and_only_builds_the_backfill_reader() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("prefixed.raw");
    let volume = fixture::image(96);
    let mut image = vec![0; 512];
    image.extend_from_slice(&volume);
    std::fs::write(&path, image).unwrap();
    let mut plan = candidate();
    plan.offset = 512;
    plan.length = Some(volume.len() as u64);
    plan.source = ImageFilesystemSource::MbrPartition;
    let work =
        build_partition_work(&path, &DataSourceKind::Raw, 1, "NTFS", "ntfs", &[plan]).unwrap();
    let profiler = ImportProfiler::default();
    let result = enumerate_single_partition_profiled(
        temp.path(),
        "prefix-source",
        work,
        &AtomicBool::new(false),
        None,
        Some(profiler.clone()),
    );
    assert!(result.error.is_none(), "{:?}", result.error);
    let report = profiler.report();
    let backfill = report
        .samples
        .iter()
        .find(|sample| sample.phase == Phase::MftDirReaderBuild)
        .unwrap();
    assert_eq!(backfill.filesystem_builds, 1);
    assert!(!report
        .samples
        .iter()
        .any(|sample| sample.phase == Phase::FallbackReaderBuild));
}

#[test]
fn unusable_mft_parameters_build_one_fallback_reader_and_clear_old_rows() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("fallback.E01");
    let mut bytes = fixture::image(96);
    let mut attr = fixture::MFT_OFFSET + 56;
    while u32::from_le_bytes(bytes[attr..attr + 4].try_into().unwrap()) != 0x80 {
        attr += u32::from_le_bytes(bytes[attr + 4..attr + 8].try_into().unwrap()) as usize;
    }
    bytes[attr + 48..attr + 56].copy_from_slice(&0u64.to_le_bytes());
    fixture::write_e01(&path, &bytes).unwrap();
    let conn = staging::open_partition_staging(temp.path(), "fallback-source", 1).unwrap();
    insert_staging_entry(&conn, "old-mft-row", "fallback-source");
    staging::set_staging_meta(&conn, "enum_strategy", "mft").unwrap();
    drop(conn);
    let profiler = ImportProfiler::default();
    let result = enumerate_single_partition_profiled(
        temp.path(),
        "fallback-source",
        deferred(&path, DataSourceKind::E01),
        &AtomicBool::new(false),
        None,
        Some(profiler.clone()),
    );
    let conn = staging::open_partition_staging(temp.path(), "fallback-source", 1).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM file_entries WHERE id = 'old-mft-row'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        staging::get_staging_meta(&conn, "enum_strategy").unwrap(),
        None
    );
    let report = profiler.report();
    let fallback = report
        .samples
        .iter()
        .find(|sample| sample.phase == Phase::FallbackReaderBuild)
        .unwrap();
    assert_eq!(fallback.filesystem_builds, 1);
}

fn fake_ntfs(path: PathBuf, active: Arc<AtomicUsize>, max: Arc<AtomicUsize>) -> PartitionWork {
    let mut work = fake_partition_work(0, 3, Duration::ZERO, active, max);
    work.fs_kind = "ntfs".into();
    work.source_path = path;
    work
}

#[test]
fn cancelled_fast_path_does_not_start_recursive_enumeration() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = Arc::new(AtomicUsize::new(0));
    let max = Arc::new(AtomicUsize::new(0));
    let work = fake_ntfs(temp.path().join("missing.raw"), active, max.clone());
    let profiler = ImportProfiler::default();
    let result = enumerate_single_partition_profiled(
        temp.path(),
        "cancel-source",
        work,
        &AtomicBool::new(true),
        None,
        Some(profiler.clone()),
    );
    assert_eq!(result.error.as_deref(), Some("Cancelled"));
    assert!(result.warnings.is_empty());
    assert_eq!(max.load(Ordering::SeqCst), 0);
    let report = profiler.report();
    let enumeration = report
        .samples
        .iter()
        .find(|sample| sample.phase == Phase::Enumeration)
        .unwrap();
    assert_eq!(enumeration.state, State::Cancelled);
    assert_eq!(enumeration.failures, 0);
}

#[test]
fn sqlite_stage_failure_rolls_back_without_recursive_fallback() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("stage-error.raw");
    std::fs::write(&path, fixture::image(96)).unwrap();
    let conn = staging::open_partition_staging(temp.path(), "db-source", 0).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_insert BEFORE INSERT ON ntfs_entries BEGIN SELECT RAISE(ABORT, 'synthetic stage failure'); END").unwrap();
    drop(conn);
    let max = Arc::new(AtomicUsize::new(0));
    let work = fake_ntfs(path, Arc::new(AtomicUsize::new(0)), max.clone());
    let result = enumerate_single_partition(
        temp.path(),
        "db-source",
        work,
        &AtomicBool::new(false),
        None,
    );
    let conn = staging::open_partition_staging(temp.path(), "db-source", 0).unwrap();
    assert!(result.error.unwrap().contains("Database error"));
    assert_eq!(max.load(Ordering::SeqCst), 0);
    assert!(result.warnings.is_empty());
    assert_eq!(staging::staging_db_row_count(&conn).unwrap(), 0);
}

#[test]
fn failed_fallback_cleanup_does_not_invoke_the_recursive_reader() {
    let temp = tempfile::TempDir::new().unwrap();
    let conn = staging::open_partition_staging(temp.path(), "cleanup-source", 0).unwrap();
    insert_staging_entry(&conn, "old-row", "cleanup-source");
    staging::set_staging_meta(&conn, "enum_strategy", "mft").unwrap();
    conn.execute_batch("CREATE TRIGGER reject_cleanup BEFORE DELETE ON staging_meta BEGIN SELECT RAISE(ABORT, 'synthetic cleanup failure'); END").unwrap();
    drop(conn);
    let max = Arc::new(AtomicUsize::new(0));
    let work = fake_ntfs(
        temp.path().join("missing.raw"),
        Arc::new(AtomicUsize::new(0)),
        max.clone(),
    );
    let result = enumerate_single_partition(
        temp.path(),
        "cleanup-source",
        work,
        &AtomicBool::new(false),
        None,
    );
    let conn = staging::open_partition_staging(temp.path(), "cleanup-source", 0).unwrap();
    assert!(result.error.unwrap().contains("Staging persistence error"));
    assert_eq!(max.load(Ordering::SeqCst), 0);
    assert_eq!(staging::staging_db_row_count(&conn).unwrap(), 1);
}

#[test]
fn completion_marker_failure_is_a_partition_failure() {
    let temp = tempfile::TempDir::new().unwrap();
    let conn = staging::open_partition_staging(temp.path(), "completion-source", 0).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_done BEFORE INSERT ON staging_meta WHEN NEW.key = 'status' AND NEW.value = 'done' BEGIN SELECT RAISE(ABORT, 'synthetic completion failure'); END").unwrap();
    drop(conn);
    let work = fake_partitions(1, 3, Duration::ZERO).pop().unwrap();
    let profiler = ImportProfiler::default();
    let result = enumerate_single_partition_profiled(
        temp.path(),
        "completion-source",
        work,
        &AtomicBool::new(false),
        None,
        Some(profiler.clone()),
    );
    let conn = staging::open_partition_staging(temp.path(), "completion-source", 0).unwrap();
    assert!(result
        .error
        .unwrap()
        .contains("Failed to complete partition staging"));
    assert_eq!(
        staging::get_staging_meta(&conn, "status")
            .unwrap()
            .as_deref(),
        Some("failed")
    );
    assert_eq!(profiler.report().samples[0].state, State::Failed);
}

#[test]
fn invalid_deferred_offset_is_rejected_before_reader_initialization() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut work = deferred(&temp.path().join("missing.raw"), DataSourceKind::Raw);
    work.volume_offset = 512;
    let profiler = ImportProfiler::default();
    let result = enumerate_single_partition_profiled(
        temp.path(),
        "invalid-source",
        work,
        &AtomicBool::new(false),
        None,
        Some(profiler.clone()),
    );
    assert!(result.error.unwrap().contains("Invalid deferred NTFS"));
    assert!(result.warnings.is_empty());
    assert!(!profiler
        .report()
        .samples
        .iter()
        .any(|sample| sample.phase == Phase::MftReaderOpen));
}

#[test]
fn cancellation_after_a_parallel_batch_rolls_back_and_never_starts_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("parallel-cancel.raw");
    std::fs::write(&path, fixture::image(20_003)).unwrap();
    let mut work = deferred(&path, DataSourceKind::Raw);
    work.reader_factory =
        Some(crate::source_runtime::open_factory(&path, &DataSourceKind::Raw).unwrap());
    work.mft_workers = 3;
    let cancel = AtomicBool::new(false);
    let progress = |_, _| {
        cancel.store(true, Ordering::SeqCst);
    };
    let result = enumerate_single_partition(
        temp.path(),
        "cancel-parallel",
        work,
        &cancel,
        Some(&progress),
    );
    assert!(result.error.unwrap().contains("Cancelled"));
    assert!(result.warnings.is_empty());
    let conn = staging::open_partition_staging(temp.path(), "cancel-parallel", 0).unwrap();
    assert_eq!(staging::staging_db_row_count(&conn).unwrap(), 0);
    assert_ne!(
        staging::get_staging_meta(&conn, "status")
            .unwrap()
            .as_deref(),
        Some("done")
    );
}
