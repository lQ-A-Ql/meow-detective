use app_services::derived_source_service::MaterializedRbdSource;

use super::types::{BackgroundDerivedSourceProcessingJob, BackgroundLinuxEvidenceSetImportJob};

pub(super) fn build_derived_processing_job(
    job: &BackgroundLinuxEvidenceSetImportJob,
    derived_sources: Vec<MaterializedRbdSource>,
) -> BackgroundDerivedSourceProcessingJob {
    BackgroundDerivedSourceProcessingJob {
        db_path: job.db_path.clone(),
        case_id: job.case_id.clone(),
        case_root: job.case_root.clone(),
        import_set_id: job.plan.import_set_id.clone(),
        source_ids: derived_sources
            .into_iter()
            .map(|source| source.data_source.id)
            .collect(),
    }
}
