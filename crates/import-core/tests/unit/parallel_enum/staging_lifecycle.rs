use super::*;
use persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo as Repo;

fn run(
    c: &rusqlite::Connection,
    path: &std::path::Path,
    source: &str,
    partition: usize,
    cancel: &AtomicBool,
    progress: Option<&dyn Fn(u64, u64)>,
) -> Result<EnumerationStats, ParallelEnumError> {
    crate::parallel_enum::ntfs::enumerate_ntfs_reader_to_staging(
        c,
        Box::new(evidence_core::RawImageReader::open(path).unwrap()),
        source,
        partition,
        0,
        cancel,
        progress,
    )
}
fn assert_source(c: &rusqlite::Connection, source: &str, partition: usize, rows: u64) {
    assert_eq!(Repo::meta(c, "source_id").unwrap().as_deref(), Some(source));
    assert_eq!(
        Repo::meta(c, "partition").unwrap().as_deref(),
        Some(partition.to_string().as_str())
    );
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM file_entries", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        rows
    );
}
#[test]
fn cancelled_rerun_rolls_back_old_catalog_and_retry_replaces_identity_and_counts() {
    let temp = tempfile::tempdir().unwrap();
    let c = staging::open_partition_staging(temp.path(), "lifecycle", 1).unwrap();
    let small = temp.path().join("small.raw");
    std::fs::write(&small, ntfs_fixture::image(96)).unwrap();
    let big = temp.path().join("big.raw");
    std::fs::write(&big, ntfs_fixture::image(20_001)).unwrap();
    let cancel = AtomicBool::new(false);
    let first = run(&c, &small, "old", 1, &cancel, None).unwrap();
    assert_eq!(first.file_count + first.dir_count, 82);
    let failed = run(
        &c,
        &big,
        "cancelled",
        7,
        &cancel,
        Some(&|_, _| cancel.store(true, Ordering::Release)),
    );
    assert!(matches!(failed, Err(ParallelEnumError::Cancelled)));
    assert!(c.is_autocommit());
    assert_source(&c, "old", 1, 82);
    cancel.store(false, Ordering::Release);
    let retried = run(&c, &small, "new", 9, &cancel, None).unwrap();
    assert_eq!(retried.file_count + retried.dir_count, 82);
    assert_source(&c, "new", 9, 82);
}
#[test]
fn deferred_commit_failure_keeps_previous_rows_and_unpublished_counts() {
    let temp = tempfile::tempdir().unwrap();
    let c = staging::open_partition_staging(temp.path(), "commit", 1).unwrap();
    let path = temp.path().join("disk.raw");
    std::fs::write(&path, ntfs_fixture::image(96)).unwrap();
    let cancel = AtomicBool::new(false);
    run(&c, &path, "old", 1, &cancel, None).unwrap();
    c.execute_batch("CREATE TABLE commit_parent(id INTEGER PRIMARY KEY); CREATE TABLE commit_child(id INTEGER REFERENCES commit_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fail_commit AFTER INSERT ON ntfs_entries WHEN NEW.record_num=16 BEGIN INSERT INTO commit_child VALUES(99); END;").unwrap();
    assert!(run(&c, &path, "failed", 2, &cancel, None).is_err());
    assert!(c.is_autocommit());
    assert_source(&c, "old", 1, 82);
    c.execute_batch("DROP TRIGGER fail_commit").unwrap();
    let retry = run(&c, &path, "retry", 3, &cancel, None).unwrap();
    assert_eq!(retry.file_count + retry.dir_count, 82);
    assert_source(&c, "retry", 3, 82);
}
#[test]
fn controlled_disk_full_rolls_back_then_allows_retry() {
    let temp = tempfile::tempdir().unwrap();
    let c = staging::open_partition_staging(temp.path(), "full", 1).unwrap();
    let small = temp.path().join("small.raw");
    std::fs::write(&small, ntfs_fixture::image(96)).unwrap();
    let cancel = AtomicBool::new(false);
    run(&c, &small, "old", 1, &cancel, None).unwrap();
    let pages: u64 = c.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap();
    c.execute_batch(&format!("PRAGMA max_page_count={}", pages + 2))
        .unwrap();
    let big = temp.path().join("big.raw");
    std::fs::write(&big, ntfs_fixture::image(20_001)).unwrap();
    let failure = run(&c, &big, "full", 2, &cancel, None).unwrap_err();
    assert!(
        matches!(failure,ParallelEnumError::Db(rusqlite::Error::SqliteFailure(e,_)) if e.code==rusqlite::ErrorCode::DiskFull)
    );
    assert!(c.is_autocommit());
    assert_source(&c, "old", 1, 82);
    c.execute_batch("PRAGMA max_page_count=1000000").unwrap();
    run(&c, &small, "retry", 3, &cancel, None).unwrap();
    assert_source(&c, "retry", 3, 82);
}
