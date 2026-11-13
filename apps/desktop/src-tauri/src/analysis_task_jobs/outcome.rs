use app_services::analysis_service::AnalysisExtractionExecution;
use domain::JobId;
use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::{
    dto::{AnalysisExtractionRunDto, AnalysisParseStatusDto},
    CommandError,
};

pub(crate) struct AnalysisTaskOutcome<T> {
    pub(super) value: T,
    warning_count: u32,
    failed_count: u32,
}

impl<T> AnalysisTaskOutcome<T> {
    pub(crate) fn with_counts(value: T, warning_count: u32, failed_count: u32) -> Self {
        Self {
            value,
            warning_count,
            failed_count,
        }
    }
}

impl AnalysisTaskOutcome<AnalysisExtractionRunDto> {
    pub(crate) fn extraction(execution: AnalysisExtractionExecution) -> Self {
        let failed_count = execution.retryable_failure_count.max(u64::from(
            execution.dto.status == AnalysisParseStatusDto::Failed,
        ));
        let warning_count = u32::try_from(execution.dto.warnings.len()).unwrap_or(u32::MAX);
        Self::with_counts(
            execution.dto,
            warning_count,
            u32::try_from(failed_count).unwrap_or(u32::MAX),
        )
    }
}

pub(super) fn settle<T>(
    repo: &JobRepo<'_>,
    job: &JobId,
    kind: &str,
    app: Option<&AppHandle>,
    outcome: &AnalysisTaskOutcome<T>,
) -> Result<(), CommandError> {
    repo.update_outcome_counts(
        job,
        outcome.warning_count,
        0,
        outcome.failed_count,
        outcome.warning_count > 0 || outcome.failed_count > 0,
    )
    .map_err(app_services::connection::command_error_from_db)?;
    if outcome.failed_count > 0 {
        let detail = format!(
            "{kind} finished with {} failed candidates",
            outcome.failed_count
        );
        repo.fail_interrupted(job, &detail)
            .map_err(app_services::connection::command_error_from_db)?;
        if let Some(app) = app {
            crate::events::event_bridge::emit_job_failed(app, &job.0, &detail);
        }
    } else {
        let detail = format!("{kind} completed");
        repo.complete_committed(job, &detail)
            .map_err(app_services::connection::command_error_from_db)?;
        super::emit_completed(app, &job.0, &detail);
    }
    Ok(())
}
