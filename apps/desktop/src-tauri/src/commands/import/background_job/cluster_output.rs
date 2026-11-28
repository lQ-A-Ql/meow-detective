use super::types::{BackgroundDerivedSourceProcessingJob, BackgroundLinuxEvidenceSetImportJob};

pub(super) fn build_derived_processing_job(
    job: &BackgroundLinuxEvidenceSetImportJob,
    mut source_ids: Vec<domain::DataSourceId>,
) -> BackgroundDerivedSourceProcessingJob {
    source_ids.sort_by(|left, right| left.0.cmp(&right.0));
    source_ids.dedup();
    BackgroundDerivedSourceProcessingJob {
        db_path: job.db_path.clone(),
        case_id: job.case_id.clone(),
        case_root: job.case_root.clone(),
        import_set_id: job.plan.import_set_id.clone(),
        source_ids,
    }
}
