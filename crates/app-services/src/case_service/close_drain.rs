use super::Result;
use persistence_sqlite::repositories::job_repo::JobRepo;
use rusqlite::Connection;

/// Result of draining running jobs during case close.
#[derive(Debug, Clone)]
pub struct DrainResult {
    /// Whether all jobs drained completely within the timeout.
    pub fully_drained: bool,
    /// IDs of jobs that were still pending and had to be marked as interrupted.
    pub pending_jobs: Vec<String>,
    /// Human-readable warnings about jobs that did not drain in time.
    pub warnings: Vec<String>,
}

/// Finalise database job state during case close.
///
/// After the caller has cancelled all background tasks via `TaskManager` and
/// waited for them to stop, this function checks the database for any jobs
/// still left in `running` or `cancelling` state. Those jobs are marked as
/// `failed` with reason `interrupted_during_close`.
///
/// The `timeout_ms` parameter documents the drain window that was used by the
/// caller; it is recorded in the job detail for diagnostics.
///
/// The database connection is not closed by this function. The caller owns
/// releasing the connection pool afterwards.
pub fn close_case_drain(conn: &Connection, _case_id: &str, timeout_ms: u64) -> Result<DrainResult> {
    let repo = JobRepo::new(conn);
    let interrupted = repo.find_interrupted()?;

    let mut pending_jobs = Vec::with_capacity(interrupted.len());
    let mut warnings = Vec::with_capacity(interrupted.len());

    for job_id in &interrupted {
        let detail = format!("interrupted_during_close (drain timeout {}ms)", timeout_ms);
        match repo.fail(job_id, &detail) {
            Ok(()) => {
                warnings.push(format!(
                    "Job {} was still running after {}ms drain timeout — marked as failed",
                    job_id.0, timeout_ms
                ));
            }
            Err(e) => {
                warnings.push(format!(
                    "Job {} still running after {}ms drain timeout, but failed to mark as interrupted: {}",
                    job_id.0, timeout_ms, e
                ));
            }
        }
        pending_jobs.push(job_id.0.clone());
    }

    Ok(DrainResult {
        fully_drained: interrupted.is_empty(),
        pending_jobs,
        warnings,
    })
}
