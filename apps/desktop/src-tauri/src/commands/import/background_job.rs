//! Background import job lifecycle facade.

mod cluster;
mod cluster_members;
mod cluster_output;
mod cluster_presence;
mod cluster_status;
mod evidence_hash;
mod gate;
mod single;
mod status;
mod types;

pub(crate) use cluster::run_background_linux_cluster_import_until_browseable;
pub(crate) use cluster_status::{
    cancel_browseable_cluster_job, complete_browseable_cluster_job,
    continue_cluster_rbd_processing, fail_browseable_cluster_job,
};
pub(crate) use evidence_hash::schedule_pending_evidence_hashes;
pub(crate) use single::run_background_import_job;
pub(crate) use types::{
    BackgroundDerivedSourceProcessingJob, BackgroundImportJob, BackgroundLinuxClusterImportJob,
};

#[cfg(test)]
#[path = "../../../tests/unit/commands/import/background_job.rs"]
mod tests;
