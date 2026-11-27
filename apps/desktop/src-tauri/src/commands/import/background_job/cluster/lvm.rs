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

pub(super) enum LvmOutcome {
    Ready(Vec<domain::DataSource>),
    Cancelled,
    Failed(CommandError),
}

pub(super) fn materialize_logical_volumes(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
    cancel_token: &AtomicBool,
) -> LvmOutcome {
    if let Some(app) = app {
        event_bridge::emit_job_progress(
            app,
            &job.job_id.0,
            85,
            "Materializing LVM logical volume catalogs",
        );
    }
    let Some(collection) = summary.collection_plan.as_ref() else {
        return LvmOutcome::Failed(CommandError::internal("Collection image plan is missing"));
    };
    match cluster_service::materialize_linux_evidence_set_lvm_with_cancel(
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
                "lvm_catalog",
                true,
                None,
                summary.ready_count,
                summary.failed_count,
            );
            if let Some(app) = app {
                event_bridge::emit_job_progress(
                    app,
                    &job.job_id.0,
                    92,
                    "LVM logical volume catalogs are ready",
                );
            }
            LvmOutcome::Ready(sources)
        }
        Err(_) if cancel_token.load(Ordering::Acquire) => {
            record_lvm_cancellation(connection, job_repo, job, app, summary);
            LvmOutcome::Cancelled
        }
        Err(error) => {
            let detail = format!(
                "Linux evidence-set import failed during LVM logical volume materialization: {error}"
            );
            record_cluster_phase(
                connection,
                job,
                "lvm_catalog",
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
                "Failed to mark Linux evidence set failed after LVM materialization failure",
            );
            LvmOutcome::Failed(CommandError::from_typed_service_error(error))
        }
    }
}

pub(super) fn record_lvm_cancellation(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
) {
    record_cluster_phase(
        connection,
        job,
        "lvm_catalog",
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
        Some("Linux evidence-set import cancelled during LVM logical volume materialization"),
        "Failed to mark Linux evidence set cancelled after LVM cancellation",
    );
    cancel_job(
        job_repo,
        &job.job_id,
        app,
        "Linux evidence-set import cancelled during LVM logical volume materialization",
    );
}
