use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;

use super::super::super::cancellation::is_import_cancelled_message;
use super::super::status::cancel_job;
use super::super::types::ClusterImportSummary;
use super::queue::cancel_member_job;
use super::types::{update_cluster_progress, MemberCoordinator, MemberFailureAction, MemberResult};
use crate::events::event_bridge;

impl<'a, 'db> MemberCoordinator<'a, 'db> {
    pub(super) fn collect_member_results(
        &self,
        result_rx: Receiver<MemberResult>,
        summary: &mut ClusterImportSummary,
    ) -> Result<(), super::CommandError> {
        let mut cancelled = false;
        let mut completed = 0u32;
        let mut first_error = None;
        while let Ok(result) = result_rx.recv() {
            completed = completed.saturating_add(1);
            cancelled |= self.cancel_token.load(Ordering::Acquire);
            match result.outcome {
                Ok(message) => {
                    if let Err(error) =
                        self.record_member_success(summary, result.index, result.job_id, message)
                    {
                        tracing::error!(
                            member_index = result.index,
                            error = %error.message,
                            "Failed to record completed Linux cluster member"
                        );
                        first_error.get_or_insert(error);
                    } else if !cancelled {
                        if let Err(error) = self.emit_member_progress(
                            completed,
                            &format!("member {}", result.index + 1),
                        ) {
                            first_error.get_or_insert(error);
                        }
                    }
                }
                Err(error) => {
                    if cancelled || is_import_cancelled_message(&error.message) {
                        self.cancel_token.store(true, Ordering::Release);
                        cancelled = true;
                        cancel_member_job(self.job_repo, self.app, &result.job_id, &error.message);
                    } else if matches!(
                        self.handle_member_failure(summary, result.index, result.job_id, error),
                        MemberFailureAction::StopCancelled
                    ) {
                        self.cancel_token.store(true, Ordering::Release);
                        cancelled = true;
                    }
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn emit_member_progress(
        &self,
        completed: u32,
        source_name: &str,
    ) -> Result<(), super::CommandError> {
        let progress = 10 + (completed.saturating_mul(80) / self.total_members.max(1));
        let detail = format!(
            "Importing Linux cluster member {}/{}: {}",
            completed, self.total_members, source_name
        );
        self.job_repo
            .update_progress(&self.job.job_id, progress.min(90), &detail)
            .map_err(super::CommandError::from_typed_service_error)?;
        if let Some(app) = self.app {
            event_bridge::emit_job_progress(app, &self.job.job_id.0, progress.min(90), &detail);
        }
        Ok(())
    }

    fn record_member_success(
        &self,
        summary: &mut ClusterImportSummary,
        member_index: usize,
        member_job_id: domain::JobId,
        message: String,
    ) -> Result<(), super::CommandError> {
        self.job_repo
            .complete(&member_job_id, &message)
            .map_err(super::CommandError::from_typed_service_error)?;
        if let Some(app) = self.app {
            event_bridge::emit_job_completed(app, &member_job_id.0, &message);
        }
        summary.ready_count = summary.ready_count.saturating_add(1);
        summary
            .member_messages
            .push(format!("member {}: {message}", member_index + 1));
        update_cluster_progress(self.connection, self.job, summary)
    }

    fn handle_member_failure(
        &self,
        summary: &mut ClusterImportSummary,
        member_index: usize,
        member_job_id: domain::JobId,
        error: super::CommandError,
    ) -> MemberFailureAction {
        summary.failed_count = summary.failed_count.saturating_add(1);
        summary
            .member_messages
            .push(format!("member {}: {}", member_index + 1, error.message));
        if is_import_cancelled_message(&error.message) {
            cancel_member_job(self.job_repo, self.app, &member_job_id, &error.message);
        } else if let Err(update_error) = self.job_repo.fail(&member_job_id, &error.message) {
            tracing::warn!(error = %update_error, "Failed to mark cluster member job as failed");
            if let Some(app) = self.app {
                event_bridge::emit_job_failed(app, &member_job_id.0, &error.message);
            }
        } else if let Some(app) = self.app {
            event_bridge::emit_job_failed(app, &member_job_id.0, &error.message);
        }
        let _ = update_cluster_progress_with_error(self, summary, &error.message);
        if is_import_cancelled_message(&error.message) {
            cancel_job(self.job_repo, &self.job.job_id, self.app, &error.message);
            MemberFailureAction::StopCancelled
        } else {
            tracing::warn!(
                cluster_id = %self.job.plan.cluster_id,
                ready_count = summary.ready_count,
                failed_count = summary.failed_count,
                error = %error.message,
                "Linux cluster member import failed; continuing with remaining members"
            );
            MemberFailureAction::Continue
        }
    }
}

fn update_cluster_progress_with_error(
    coordinator: &MemberCoordinator<'_, '_>,
    summary: &ClusterImportSummary,
    error: &str,
) -> Result<(), super::CommandError> {
    app_services::cluster_service::update_linux_cluster_import_state(
        coordinator.connection,
        &coordinator.job.plan.cluster_id,
        "importing",
        summary.ready_count,
        summary.failed_count,
        Some(error),
    )
    .map_err(super::CommandError::from_typed_service_error)
}

pub(super) fn cancel_cluster_members(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &super::super::types::BackgroundLinuxClusterImportJob,
    app: Option<&super::AppHandle>,
    summary: &ClusterImportSummary,
) {
    let message = "Linux cluster import cancelled by user";
    let _ = app_services::cluster_service::update_linux_cluster_import_state(
        connection,
        &job.plan.cluster_id,
        "cancelled",
        summary.ready_count,
        summary.failed_count,
        Some(message),
    );
    cancel_job(job_repo, &job.job_id, app, message);
}
