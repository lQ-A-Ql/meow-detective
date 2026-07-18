use app_services::ceph_reconstruction::MaterializedRbdSource;

use super::types::{BackgroundDerivedSourceProcessingJob, BackgroundLinuxClusterImportJob};

pub(super) fn build_derived_processing_job(
    job: &BackgroundLinuxClusterImportJob,
    derived_sources: Vec<MaterializedRbdSource>,
) -> BackgroundDerivedSourceProcessingJob {
    BackgroundDerivedSourceProcessingJob {
        db_path: job.db_path.clone(),
        case_id: job.case_id.clone(),
        case_root: job.case_root.clone(),
        cluster_id: job.plan.cluster_id.clone(),
        source_ids: derived_sources
            .into_iter()
            .map(|source| source.data_source.id)
            .collect(),
    }
}
