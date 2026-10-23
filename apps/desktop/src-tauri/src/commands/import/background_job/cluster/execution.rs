use std::sync::{atomic::AtomicBool, Arc};

use tauri::AppHandle;
use transport::CommandError;

use super::completion::complete_evidence_set_import;
use super::ledger::record_cluster_phase;
use super::JobRepo;
use crate::commands::import::background_job::cluster_members::import_evidence_set_members;
use crate::commands::import::background_job::types::{
    BackgroundLinuxEvidenceSetImportJob, BrowseableEvidenceSetImport, EvidenceSetImportSummary,
};
use crate::state::TaskManager;

pub(super) fn import_members(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    cancel_token: &Arc<AtomicBool>,
    task_manager: Option<Arc<TaskManager>>,
) -> Result<Option<EvidenceSetImportSummary>, CommandError> {
    match import_evidence_set_members(connection, job_repo, job, app, cancel_token, task_manager) {
        Ok(summary) => Ok(summary),
        Err(error) => {
            record_cluster_phase(
                connection,
                job,
                "member_import",
                false,
                Some(&error.message),
                0,
                0,
            );
            Err(error)
        }
    }
}

pub(super) fn publish_evidence_set(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: EvidenceSetImportSummary,
    cancel_token: Arc<AtomicBool>,
    projection_task_manager: Option<Arc<TaskManager>>,
) -> Result<Option<BrowseableEvidenceSetImport>, CommandError> {
    match complete_evidence_set_import(connection, job_repo, job, app, summary, cancel_token) {
        Ok(outcome) => {
            if outcome.is_some() {
                record_cluster_phase(
                    connection,
                    job,
                    "publish",
                    true,
                    None,
                    job.plan.members.len() as u32,
                    0,
                );
                if let (Some(task_manager), Some(app)) = (projection_task_manager.as_ref(), app) {
                    crate::case_projection_jobs::schedule(
                        task_manager,
                        app,
                        &job.case_id.0,
                        &job.case_root,
                    )?;
                }
            }
            Ok(outcome)
        }
        Err(error) => {
            record_cluster_phase(
                connection,
                job,
                "publish",
                false,
                Some(&error.message),
                0,
                1,
            );
            Err(error)
        }
    }
}
