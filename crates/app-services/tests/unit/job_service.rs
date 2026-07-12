use super::{
    cancel_job, get_jobs_from_db, get_trace_items_from_db, get_warnings_from_db,
    parse_partition_progress, recover_interrupted_jobs,
};
use persistence_sqlite::repositories::job_repo::JobRepo;

#[test]
fn parses_partition_progress_payload() {
    let meta = parse_partition_progress(
        "[partition-progress] 1|5|42|Partition 3 (NTFS) - Basic data partition|Enumerating Partition 3 (NTFS) - Basic data partition",
    )
    .expect("expected metadata");

    assert_eq!(meta.completed_partitions, 1);
    assert_eq!(meta.total_partitions, 5);
    assert_eq!(meta.partition_progress, 42);
    assert_eq!(
        meta.current_partition.as_deref(),
        Some("Partition 3 (NTFS) - Basic data partition")
    );
    assert_eq!(meta.scope.as_deref(), Some("分区 2/5"));
    assert_eq!(
        meta.detail,
        "Enumerating Partition 3 (NTFS) - Basic data partition"
    );
}

#[test]
fn maps_job_partial_counts_from_repository() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, number, examiner) VALUES ('case-1', 'Case', '1', 'qa')",
        [],
    )
    .unwrap();

    let repo = JobRepo::new(&conn);
    let job_id = repo.create("case-1", "Import data source").unwrap();
    repo.update_outcome_counts(&job_id, 2, 3, 0, true).unwrap();
    repo.complete(&job_id, "Completed with warnings").unwrap();

    let jobs = get_jobs_from_db(&conn).unwrap();
    let snapshot = jobs.iter().find(|job| job.id == job_id.0).unwrap();

    assert_eq!(snapshot.warning_count, 2);
    assert_eq!(snapshot.skipped_count, 3);
    assert_eq!(snapshot.failed_count, 0);
    assert!(snapshot.partial);
    assert_eq!(snapshot.status, "completed");
}

#[test]
fn derives_partial_when_counts_are_non_zero() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, number, examiner) VALUES ('case-1', 'Case', '1', 'qa')",
        [],
    )
    .unwrap();

    let repo = JobRepo::new(&conn);
    let job_id = repo.create("case-1", "Search index").unwrap();
    repo.update_outcome_counts(&job_id, 1, 0, 0, false).unwrap();
    repo.complete(&job_id, "Completed with one warning")
        .unwrap();

    let jobs = get_jobs_from_db(&conn).unwrap();
    let snapshot = jobs.iter().find(|job| job.id == job_id.0).unwrap();

    assert!(snapshot.partial);
    assert_eq!(snapshot.warning_count, 1);
}

#[test]
fn cancel_job_sets_status_to_cancelled() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, number, examiner) VALUES ('case-1', 'Case', '1', 'qa')",
        [],
    )
    .unwrap();

    let repo = JobRepo::new(&conn);
    let job_id = repo.create("case-1", "Import data source").unwrap();

    cancel_job(&conn, &job_id, "Import cancelled by user").unwrap();

    let jobs = get_jobs_from_db(&conn).unwrap();
    let snapshot = jobs.iter().find(|job| job.id == job_id.0).unwrap();
    assert_eq!(snapshot.status, "cancelled");
}

#[test]
fn recover_interrupted_jobs_marks_running_and_cancelling_as_failed() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, number, examiner) VALUES ('case-1', 'Case', '1', 'qa')",
        [],
    )
    .unwrap();

    let repo = JobRepo::new(&conn);

    // Create a running job (simulates interrupted import)
    let running_id = repo.create("case-1", "Import data source").unwrap();

    // Create a cancelling job (simulates interrupted cancel)
    let cancelling_id = repo.create("case-1", "Import data source").unwrap();
    repo.mark_cancelling(&cancelling_id, "Cancel requested by user")
        .unwrap();

    // Create a completed job (should not be touched)
    let completed_id = repo.create("case-1", "Import data source").unwrap();
    repo.complete(&completed_id, "Done").unwrap();

    // Create a failed job (should remain failed)
    let already_failed_id = repo.create("case-1", "Index rebuild").unwrap();
    repo.fail(&already_failed_id, "disk full").unwrap();

    let result = recover_interrupted_jobs(&conn).unwrap();

    // Only running + cancelling should be recovered
    assert_eq!(result.recovered_job_ids.len(), 2);
    assert!(result.recovered_job_ids.contains(&running_id.0));
    assert!(result.recovered_job_ids.contains(&cancelling_id.0));

    // Verify status changes
    let jobs = get_jobs_from_db(&conn).unwrap();

    let running_snapshot = jobs.iter().find(|job| job.id == running_id.0).unwrap();
    assert_eq!(running_snapshot.status, "failed");
    assert!(running_snapshot.detail.contains("Interrupted"));

    let cancelling_snapshot = jobs.iter().find(|job| job.id == cancelling_id.0).unwrap();
    assert_eq!(cancelling_snapshot.status, "failed");
    assert!(cancelling_snapshot.detail.contains("Interrupted"));

    // Completed job unchanged
    let completed_snapshot = jobs.iter().find(|job| job.id == completed_id.0).unwrap();
    assert_eq!(completed_snapshot.status, "completed");

    // Already-failed job unchanged
    let already_failed_snapshot = jobs
        .iter()
        .find(|job| job.id == already_failed_id.0)
        .unwrap();
    assert_eq!(already_failed_snapshot.status, "failed");
    assert_eq!(already_failed_snapshot.detail, "disk full");
}

#[test]
fn get_warnings_from_db_surfaces_jobs_with_nonzero_outcome_counts() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, number, examiner) VALUES ('case-1', 'Case', '1', 'qa')",
        [],
    )
    .unwrap();

    let repo = JobRepo::new(&conn);
    let clean_job = repo.create("case-1", "Search index").unwrap();
    repo.complete(&clean_job, "All good").unwrap();

    let warning_job = repo.create("case-1", "Import data source").unwrap();
    repo.update_outcome_counts(&warning_job, 2, 1, 0, true)
        .unwrap();
    repo.complete(&warning_job, "Completed with warnings")
        .unwrap();

    let warnings = get_warnings_from_db(&conn).unwrap();

    assert!(warnings.iter().all(|w| w.id != clean_job.0));
    let item = warnings
        .iter()
        .find(|w| w.id == warning_job.0)
        .expect("warning job should produce a warning item");
    assert!(item.title.contains("警告"));
    assert!(item.detail.contains("警告 2"));
    assert!(item.detail.contains("跳过 1"));
    assert!(item.detail.contains("Completed with warnings"));
}

#[test]
fn get_warnings_from_db_surfaces_failed_jobs_even_without_outcome_counts() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, number, examiner) VALUES ('case-1', 'Case', '1', 'qa')",
        [],
    )
    .unwrap();

    let repo = JobRepo::new(&conn);
    let job_id = repo.create("case-1", "Import data source").unwrap();
    repo.fail(&job_id, "disk full").unwrap();

    let warnings = get_warnings_from_db(&conn).unwrap();
    let item = warnings
        .iter()
        .find(|w| w.id == job_id.0)
        .expect("failed job should produce a warning item");
    assert!(item.title.contains("失败"));
    assert!(item.detail.contains("disk full"));
}

#[test]
fn get_trace_items_from_db_reports_one_entry_per_recent_job() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, number, examiner) VALUES ('case-1', 'Case', '1', 'qa')",
        [],
    )
    .unwrap();

    let repo = JobRepo::new(&conn);
    let job_id = repo.create("case-1", "Import data source").unwrap();
    repo.complete(&job_id, "Imported 42 files").unwrap();

    let trace = get_trace_items_from_db(&conn).unwrap();
    let item = trace
        .iter()
        .find(|t| t.id == job_id.0)
        .expect("job should produce a trace item");
    assert!(item.message.contains("Import data source"));
    assert!(item.message.contains("completed"));
    assert!(item.message.contains("Imported 42 files"));
    assert!(!item.ts.is_empty());
}
