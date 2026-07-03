use domain::JobId;
use persistence_sqlite::repositories::job_repo::{JobRepo, JobSummaryRow};
use rusqlite::Connection;
use thiserror::Error;
use transport::dto::{JobSnapshotDto, TraceItemDto, WarningItemDto};

#[derive(Debug, Error)]
pub enum JobServiceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("other error: {0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for JobServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::NotFound(_) | Self::InvalidState(_) => transport::ErrorCategory::Validation,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}

pub fn get_jobs_from_db(conn: &Connection) -> Result<Vec<JobSnapshotDto>, JobServiceError> {
    let repo = JobRepo::new(conn);
    let jobs = repo.list_recent(infrastructure::constants::JOB_LIST_LIMIT)?;
    let dtos = jobs
        .into_iter()
        .map(|job| {
            // Use DB columns for partition progress if available, fall back to parsing detail
            let has_db_partition = job.total_partitions > 0;
            let meta = if has_db_partition {
                None // DB columns are authoritative
            } else {
                parse_partition_progress(&job.detail)
            };

            let current_partition = if has_db_partition {
                job.current_partition.clone()
            } else {
                meta.as_ref()
                    .and_then(|item| item.current_partition.clone())
            };
            let completed_partitions = if has_db_partition {
                Some(job.completed_partitions)
            } else {
                meta.as_ref().map(|item| item.completed_partitions)
            };
            let total_partitions = if has_db_partition {
                Some(job.total_partitions)
            } else {
                meta.as_ref().map(|item| item.total_partitions)
            };
            let partition_progress = if has_db_partition {
                Some(job.partition_progress)
            } else {
                meta.as_ref().map(|item| item.partition_progress)
            };

            JobSnapshotDto {
                id: job.id.0,
                name: job.kind,
                scope: meta
                    .as_ref()
                    .and_then(|item| item.scope.clone())
                    .unwrap_or_else(|| {
                        if job.detail.is_empty() {
                            "Case ingest".to_string()
                        } else {
                            job.detail.clone()
                        }
                    }),
                progress: job.progress,
                status: job.status,
                detail: meta
                    .as_ref()
                    .map(|item| item.detail.clone())
                    .unwrap_or(job.detail),
                warning_count: job.warning_count,
                skipped_count: job.skipped_count,
                failed_count: job.failed_count,
                partial: job.partial
                    || job.warning_count > 0
                    || job.skipped_count > 0
                    || job.failed_count > 0,
                current_partition,
                completed_partitions,
                total_partitions,
                partition_progress,
            }
        })
        .collect();
    Ok(dtos)
}

#[derive(Debug, Clone)]
struct PartitionProgressMeta {
    scope: Option<String>,
    detail: String,
    current_partition: Option<String>,
    completed_partitions: u32,
    total_partitions: u32,
    partition_progress: u32,
}

fn parse_partition_progress(detail: &str) -> Option<PartitionProgressMeta> {
    let payload = detail.strip_prefix("[partition-progress] ")?;
    let mut parts = payload.splitn(5, '|');
    let completed: u32 = parts.next()?.parse().ok()?;
    let total: u32 = parts.next()?.parse().ok()?;
    let partition_progress: u32 = parts.next()?.parse().ok()?;
    let current_partition = parts.next()?.to_string();
    let human_detail = parts.next()?.to_string();

    Some(PartitionProgressMeta {
        scope: Some(format!(
            "分区 {}/{}",
            completed.saturating_add(1).min(total.max(1)),
            total
        )),
        detail: human_detail,
        current_partition: Some(current_partition),
        completed_partitions: completed,
        total_partitions: total,
        partition_progress,
    })
}

/// Derive BottomDrawer warning records from job outcome counters and details.
///
/// There is no persisted per-warning record store yet; this baseline surfaces
/// one warning item per job that reported a nonzero warning/skipped/failed
/// count or that failed outright, using the job's own detail text as the
/// message. This makes existing warning signals visible without requiring a
/// new schema.
pub fn get_warnings_from_db(conn: &Connection) -> Result<Vec<WarningItemDto>, JobServiceError> {
    let repo = JobRepo::new(conn);
    let jobs = repo.list_recent(infrastructure::constants::JOB_LIST_LIMIT)?;
    Ok(jobs.iter().filter_map(job_to_warning_item).collect())
}

fn job_to_warning_item(job: &JobSummaryRow) -> Option<WarningItemDto> {
    let has_outcome_warning =
        job.warning_count > 0 || job.skipped_count > 0 || job.failed_count > 0 || job.partial;
    let is_failed = job.status == "failed";
    if !has_outcome_warning && !is_failed {
        return None;
    }

    let title = if is_failed {
        format!("{} 失败", job.kind)
    } else {
        format!("{} 存在警告", job.kind)
    };
    let mut parts = Vec::new();
    if job.warning_count > 0 {
        parts.push(format!("警告 {}", job.warning_count));
    }
    if job.skipped_count > 0 {
        parts.push(format!("跳过 {}", job.skipped_count));
    }
    if job.failed_count > 0 {
        parts.push(format!("失败 {}", job.failed_count));
    }
    let counts_summary = parts.join(" · ");
    let detail = match (counts_summary.is_empty(), job.detail.is_empty()) {
        (false, false) => format!("{} — {}", counts_summary, job.detail),
        (false, true) => counts_summary,
        (true, false) => job.detail.clone(),
        (true, true) => "无详细信息".to_string(),
    };

    Some(WarningItemDto {
        id: job.id.0.clone(),
        title,
        detail,
    })
}

/// Derive a BottomDrawer trace stream from recent job lifecycle rows.
///
/// This baseline reports one trace entry per recent job using its current
/// status/detail as a coarse activity log, ordered most-recent-first.
pub fn get_trace_items_from_db(conn: &Connection) -> Result<Vec<TraceItemDto>, JobServiceError> {
    let repo = JobRepo::new(conn);
    let jobs = repo.list_recent(infrastructure::constants::JOB_LIST_LIMIT)?;
    Ok(jobs.iter().map(job_to_trace_item).collect())
}

fn job_to_trace_item(job: &JobSummaryRow) -> TraceItemDto {
    let ts = job
        .finished_at
        .clone()
        .unwrap_or_else(|| job.created_at.clone());
    let message = if job.detail.is_empty() {
        format!("[{}] {}", job.kind, job.status)
    } else {
        format!("[{}] {} — {}", job.kind, job.status, job.detail)
    };
    TraceItemDto {
        id: job.id.0.clone(),
        ts,
        message,
    }
}

/// Result of recovering interrupted jobs — returns the IDs of jobs that were
/// recovered (marked as failed after process interruption).
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    pub recovered_job_ids: Vec<String>,
}

/// Cancel a job by marking it as cancelled in the database.
///
/// This function is used when the cancel token has already been set and the
/// task has finished draining.  It finalises the job status to `cancelled`.
pub fn cancel_job(conn: &Connection, job_id: &JobId, reason: &str) -> Result<(), JobServiceError> {
    let repo = JobRepo::new(conn);
    repo.cancel(job_id, reason)?;
    Ok(())
}

/// On app restart, detect jobs that were left in `running` or `cancelling`
/// state and mark them as `failed` with reason `interrupted`.
///
/// This prevents stale jobs from appearing active after a crash or unexpected
/// shutdown.  Partial results (warning/skipped/failed counts) are preserved
/// so the user can decide whether to retry or discard the data source.
pub fn recover_interrupted_jobs(conn: &Connection) -> Result<RecoveryResult, JobServiceError> {
    let repo = JobRepo::new(conn);
    let interrupted_ids = repo
        .find_interrupted()
        .map_err(|e| JobServiceError::Other(format!("Failed to query interrupted jobs: {e}")))?;

    let recovered_job_ids: Vec<String> = interrupted_ids
        .into_iter()
        .map(|id| {
            let _ = repo.fail(&id, "Interrupted — application exited unexpectedly");
            id.0
        })
        .collect();

    Ok(RecoveryResult { recovered_job_ids })
}

#[cfg(test)]
mod tests {
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
}
