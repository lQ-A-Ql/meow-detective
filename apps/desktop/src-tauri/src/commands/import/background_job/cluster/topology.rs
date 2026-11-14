use super::super::{
    status::fail_linux_evidence_set_job,
    types::{BackgroundLinuxEvidenceSetImportJob, EvidenceSetImportSummary},
};
use tauri::AppHandle;
use transport::CommandError;

use super::ledger::record_cluster_phase;

pub(super) fn project_topology_or_fail(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
) -> Result<app_services::cluster_service::ImportSetTopologyProjection, CommandError> {
    match app_services::cluster_service::project_import_set_topology(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan.import_set_id,
    ) {
        Ok(topology) => {
            record_cluster_phase(
                connection,
                job,
                "topology_projection",
                true,
                None,
                summary.ready_count,
                summary.failed_count,
            );
            Ok(topology)
        }
        Err(error) => match fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            Some((
                connection,
                &job.plan.import_set_id,
                summary.ready_count,
                summary.failed_count,
            )),
            CommandError::from_typed_service_error(error),
        ) {
            Ok(()) => Err(CommandError::internal(
                "evidence-set topology failure did not fail its parent job",
            )),
            Err(error) => {
                record_cluster_phase(
                    connection,
                    job,
                    "topology_projection",
                    false,
                    Some(&error.message),
                    summary.ready_count,
                    summary.failed_count,
                );
                Err(error)
            }
        },
    }
}
