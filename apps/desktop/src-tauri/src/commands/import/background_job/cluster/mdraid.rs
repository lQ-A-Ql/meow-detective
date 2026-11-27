use std::sync::atomic::{AtomicBool, Ordering};

use super::super::{
    status::{cancel_job, update_import_set_state_logged},
    types::{BackgroundLinuxEvidenceSetImportJob, EvidenceSetImportSummary},
};
use crate::events::event_bridge;
use app_services::cluster_service;
use tauri::AppHandle;
use transport::CommandError;

use super::ledger::record_cluster_phase;

pub(super) enum MdRaidOutcome {
    Ready(Vec<domain::DataSource>),
    Cancelled,
    Failed(CommandError),
}

pub(super) fn materialize_nested_mdraid_catalogs(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
    cancel_token: &AtomicBool,
) -> MdRaidOutcome {
    if let Some(app) = app {
        event_bridge::emit_job_progress(
            app,
            &job.job_id.0,
            80,
            "Materializing nested MD RAID catalog",
        );
    }
    let Some(collection) = summary.collection_plan.as_ref() else {
        return MdRaidOutcome::Failed(CommandError::internal("Collection image plan is missing"));
    };
    match cluster_service::materialize_linux_evidence_set_mdraid_with_cancel(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan,
        collection,
        cancel_token,
    ) {
        Ok(sources) => {
            record_cluster_phase(
                connection,
                job,
                "mdraid_catalog",
                true,
                None,
                summary.ready_count,
                summary.failed_count,
            );
            if let Some(app) = app {
                event_bridge::emit_job_progress(
                    app,
                    &job.job_id.0,
                    90,
                    "Nested MD RAID catalog is ready",
                );
            }
            MdRaidOutcome::Ready(sources)
        }
        Err(_) if cancel_token.load(Ordering::Acquire) => {
            record_mdraid_cancellation(connection, job_repo, job, app, summary);
            MdRaidOutcome::Cancelled
        }
        Err(error) => {
            let detail = format!(
                "Linux evidence-set import failed during nested MD RAID catalog materialization: {error}"
            );
            record_cluster_phase(
                connection,
                job,
                "mdraid_catalog",
                false,
                Some(&error.to_string()),
                summary.ready_count,
                summary.failed_count,
            );
            update_import_set_state_logged(
                connection,
                &job.plan.import_set_id,
                "failed",
                summary.ready_count,
                summary.failed_count,
                Some(&detail),
                "Failed to mark Linux evidence set failed after MD RAID materialization failure",
            );
            MdRaidOutcome::Failed(CommandError::from_typed_service_error(error))
        }
    }
}

pub(super) fn record_mdraid_cancellation(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
) {
    record_cluster_phase(
        connection,
        job,
        "mdraid_catalog",
        false,
        Some("cancelled"),
        summary.ready_count,
        summary.failed_count,
    );
    update_import_set_state_logged(
        connection,
        &job.plan.import_set_id,
        "cancelled",
        summary.ready_count,
        summary.failed_count,
        Some("Linux evidence-set import cancelled during MD RAID catalog materialization"),
        "Failed to mark Linux evidence set cancelled after MD RAID cancellation",
    );
    cancel_job(
        job_repo,
        &job.job_id,
        app,
        "Linux evidence-set import cancelled during MD RAID catalog materialization",
    );
}
