use super::super::{
    status::{cancel_job, fail_linux_evidence_set_job},
    types::{BackgroundLinuxEvidenceSetImportJob, EvidenceSetImportSummary},
};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;
use transport::CommandError;

pub(super) fn materialize_pools(
    connection: &rusqlite::Connection,
    jobs: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
    cancel: &AtomicBool,
) -> Result<Option<Vec<domain::DataSource>>, CommandError> {
    let collection = summary
        .collection_plan
        .as_ref()
        .ok_or_else(|| CommandError::internal("Collection image plan is missing"))?;
    let result = app_services::cluster_service::materialize_linux_evidence_set_btrfs(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan,
        collection,
        cancel,
    );
    match result {
        Ok(sources) => {
            super::ledger::record_cluster_phase(
                connection,
                job,
                "btrfs_catalog",
                true,
                None,
                summary.ready_count,
                summary.failed_count,
            );
            Ok(Some(sources))
        }
        Err(_) if cancel.load(Ordering::Acquire) => {
            cancel_job(
                jobs,
                &job.job_id,
                app,
                "Linux evidence-set Btrfs materialization cancelled",
            );
            app_services::cluster_service::update_linux_evidence_set_import_state(
                connection,
                &job.plan.import_set_id,
                "cancelled",
                summary.ready_count,
                summary.failed_count,
                Some("Btrfs materialization cancelled"),
            )
            .map_err(CommandError::from_typed_service_error)?;
            Ok(None)
        }
        Err(error) => {
            super::ledger::record_cluster_phase(
                connection,
                job,
                "btrfs_catalog",
                false,
                Some(&error.to_string()),
                summary.ready_count,
                summary.failed_count,
            );
            fail_linux_evidence_set_job(
                jobs,
                &job.job_id,
                app,
                Some((
                    connection,
                    &job.plan.import_set_id,
                    summary.ready_count,
                    summary.failed_count,
                )),
                CommandError::from_typed_service_error(error),
            )?;
            Ok(None)
        }
    }
}
