use std::sync::{atomic::AtomicBool, Arc};

use super::super::{
    status::fail_job,
    types::{
        BackgroundLinuxEvidenceSetImportJob, BrowseableEvidenceSetImport, EvidenceSetImportSummary,
    },
};
use tauri::AppHandle;
use transport::CommandError;

use super::{
    completion_finish::{finish_ready_import, ReadyImportState},
    failures::complete_evidence_set_import_with_failures,
    lvm::{materialize_logical_volumes, LvmOutcome},
    mdraid::{materialize_nested_mdraid_catalogs, MdRaidOutcome},
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
    let lvm_sources = match materialize_logical_volumes(
        connection,
        job_repo,
        job,
        app,
        &summary,
        &cancel_token,
    ) {
        LvmOutcome::Ready(sources) => sources,
        LvmOutcome::Cancelled => return Ok(None),
        LvmOutcome::Failed(error) => {
            return fail_job(job_repo, &job.job_id, app, error).map(|()| None);
        }
    };
    let Some(btrfs_sources) =
        super::btrfs::materialize_pools(connection, job_repo, job, app, &summary, &cancel_token)?
    else {
        return Ok(None);
    };
    finish_ready_import(
        connection,
        job_repo,
        job,
        app,
        &summary,
        ReadyImportState {
            mdraid_sources,
            lvm_sources,
            btrfs_sources,
            ceph_scope_id: topology.ceph_scope_id,
        },
        cancel_token,
    )
}
