//! Background import job lifecycle facade.

mod cluster;
mod cluster_members;
mod gate;
mod single;
mod status;
mod types;

pub(crate) use cluster::run_background_linux_cluster_import_job;
pub(crate) use single::run_background_import_job;
pub(crate) use types::{BackgroundImportJob, BackgroundLinuxClusterImportJob};

#[cfg(test)]
#[path = "../../../tests/unit/commands/import/background_job.rs"]
mod tests;
