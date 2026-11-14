use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use super::super::{
    status::{fail_job, update_import_set_state_logged},
    types::{
        BackgroundLinuxEvidenceSetImportJob, BrowseableEvidenceSetImport, EvidenceSetImportSummary,
    },
};
use app_services::cluster_service;
use tauri::AppHandle;
use transport::CommandError;

use crate::events::event_bridge;

use super::{
    completion_finish::{finish_ready_import, ReadyImportState},
    failures::complete_evidence_set_import_with_failures,
    ledger::record_cluster_phase,
    mdraid::{record_mdraid_cancellation, MdRaidOutcome},
    topology::project_topology_or_fail,
};

pub(super) fn complete_evidence_set_import(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: EvidenceSetImportSummary,
    cancel_token: Arc<AtomicBool>,
) -> Result<Option<BrowseableEvidenceSetImport>, CommandError> {
    let total_members = job.plan.members.len() as u32;
    if summary.failed_count > 0 {
        return complete_evidence_set_import_with_failures(
            connection,
            job_repo,
            job,
            app,
            summary,
            total_members,
            &cancel_token,
        )
        .map(|()| None);
    }
    let topology = project_topology_or_fail(connection, job_repo, job, app, &summary)?;
    let mdraid_sources = match materialize_nested_mdraid_catalogs(
        connection,
        job_repo,
        job,
        app,
        &summary,
        &cancel_token,
    ) {
        MdRaidOutcome::Ready(sources) => sources,
        MdRaidOutcome::Cancelled => return Ok(None),
        MdRaidOutcome::Failed(error) => {
            return fail_job(job_repo, &job.job_id, app, error).map(|()| None);
        }
    };
    finish_ready_import(
        connection,
        job_repo,
        job,
        app,
        &summary,
        ReadyImportState {
            mdraid_sources,
            ceph_scope_id: topology.ceph_scope_id,
        },
        cancel_token,
    )
}

fn materialize_nested_mdraid_catalogs(
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
    match cluster_service::materialize_linux_evidence_set_mdraid_with_cancel(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan,
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
