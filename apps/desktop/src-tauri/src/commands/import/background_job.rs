//! Background import job lifecycle facade.

mod cluster;
mod cluster_members;
mod cluster_output;
mod cluster_presence;
mod cluster_status;
mod gate;
mod linux_artifacts;
mod single;
mod status;
mod types;

pub(crate) use cluster::run_background_linux_evidence_set_import_with_scheduler;
pub(crate) use cluster_status::{
    cancel_browseable_evidence_set_job, complete_browseable_evidence_set_job,
    continue_ceph_rbd_processing, fail_browseable_evidence_set_job,
};
pub(crate) use linux_artifacts::schedule_linux_artifact_analysis;
pub(crate) use single::run_background_import_job;
pub(crate) use types::{
    BackgroundDerivedSourceProcessingJob, BackgroundImportJob, BackgroundLinuxEvidenceSetImportJob,
    BrowseableEvidenceSetImport,
};

#[cfg(test)]
pub(crate) fn run_background_linux_evidence_set_import_until_browseable(
    job: BackgroundLinuxEvidenceSetImportJob,
    app: Option<&tauri::AppHandle>,
    cancel_token: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<Option<BrowseableEvidenceSetImport>, transport::CommandError> {
    cluster::run_background_linux_evidence_set_import_with_scheduler(job, app, cancel_token, None)
}

#[cfg(test)]
#[path = "../../../tests/unit/commands/import/background_job.rs"]
mod tests;
