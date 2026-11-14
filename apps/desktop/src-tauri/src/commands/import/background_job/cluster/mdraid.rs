use super::super::{
    status::{cancel_job, update_import_set_state_logged},
    types::{BackgroundLinuxEvidenceSetImportJob, EvidenceSetImportSummary},
};
use tauri::AppHandle;
use transport::CommandError;

use super::ledger::record_cluster_phase;

pub(super) enum MdRaidOutcome {
    Ready(Vec<domain::DataSource>),
    Cancelled,
    Failed(CommandError),
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
