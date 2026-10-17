//! Admission failure closes prepared worker staging just like a worker failure.
use super::{ImportAnalysisError, ImportAnalysisOptions};
use crate::resource_budget::{pool_for, ResourceError, ResourcePermit, Resources, MIB};
pub(super) fn worker_count(options: &ImportAnalysisOptions) -> usize {
    let cap = options.publication_lease.as_ref().map_or(4, |lease| {
        (1024 * MIB / (192 * MIB + lease.reader_memory_bound())).clamp(1, 4) as usize
    });
    super::resolve_analysis_worker_count(options.max_analysis_workers).min(cap)
}
pub(super) fn admit_workers(
    options: &ImportAnalysisOptions,
    workers: &[usize],
) -> Result<ResourcePermit, ImportAnalysisError> {
    let result = pool_for(options.publication_lease.as_ref()).acquire(
        Resources {
            bytes: workers.len() as u64 * 192 * MIB,
            workers: workers.len() as u64,
            ..Default::default()
        },
        Some(&options.cancel_token),
    );
    match result {
        Ok(permit) => Ok(permit),
        Err(error) => {
            let status = if matches!(error, ResourceError::Cancelled) {
                "cancelled"
            } else {
                "failed"
            };
            for worker in workers {
                let conn = crate::staging::open_analysis_staging(
                    &options.case_root,
                    &options.data_source_id.0,
                    *worker,
                )?;
                crate::staging::set_worker_meta(&conn, "status", status)?;
                crate::staging::set_worker_meta(&conn, "merged", "false")?;
            }
            Err(ImportAnalysisError::Io(error.into_io()))
        }
    }
}
