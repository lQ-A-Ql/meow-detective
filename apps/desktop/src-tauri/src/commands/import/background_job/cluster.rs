mod completion;
mod failures;
mod ledger;

use super::{
    cluster_members::import_evidence_set_members,
    gate::acquire_import_slot,
    status::{cancel_job, fail_linux_evidence_set_job},
    types::{BackgroundLinuxEvidenceSetImportJob, BrowseableEvidenceSetImport},
};
use crate::events::event_bridge;
use app_services::cluster_service;
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::AppHandle;
use transport::CommandError;

use completion::complete_evidence_set_import;
use ledger::record_cluster_phase;

pub(crate) fn run_background_linux_evidence_set_import_until_browseable(
    job: BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<Option<BrowseableEvidenceSetImport>, CommandError> {
    let connection = app_services::connection::open_case_db(&job.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&connection);
    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job.job_id.0, "Linux evidence-set import started");
        event_bridge::emit_job_progress(app, &job.job_id.0, 5, "Linux evidence-set import started");
    }
    if cancel_token.load(Ordering::Relaxed) {
        cancel_job(
            &job_repo,
            &job.job_id,
            app,
            "Linux evidence-set import cancelled by user",
        );
        return Ok(None);
    }
    let _import_slot = acquire_import_slot(&job_repo, &job.job_id, app, &cancel_token)?;
    if let Err(error) = initialize_evidence_set(&connection, &job_repo, &job, app) {
        record_cluster_phase(
            &connection,
            &job,
            "initialize",
            false,
            Some(&error.message),
            0,
            0,
        );
        return Err(error);
    }
    record_cluster_phase(&connection, &job, "initialize", true, None, 0, 0);
    let summary =
        match import_evidence_set_members(&connection, &job_repo, &job, app, &cancel_token) {
            Ok(Some(summary)) => summary,
            Ok(None) => return Ok(None),
            Err(error) => {
                record_cluster_phase(
                    &connection,
                    &job,
                    "member_import",
                    false,
                    Some(&error.message),
                    0,
                    0,
                );
                return Err(error);
            }
        };
    record_cluster_phase(
        &connection,
        &job,
        "member_import",
        summary.failed_count == 0,
        (summary.failed_count > 0).then_some("one or more members failed"),
        summary.ready_count,
        summary.failed_count,
    );
    let result =
        complete_evidence_set_import(&connection, &job_repo, &job, app, summary, cancel_token);
    match result {
        Ok(outcome) => {
            if outcome.is_some() {
                record_cluster_phase(
                    &connection,
                    &job,
                    "publish",
                    true,
                    None,
                    job.plan.members.len() as u32,
                    0,
                );
            }
            Ok(outcome)
        }
        Err(error) => {
            record_cluster_phase(
                &connection,
                &job,
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

fn initialize_evidence_set(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
) -> Result<(), CommandError> {
    if let Err(error) =
        cluster_service::register_linux_evidence_set_import(connection, &job.case_id, &job.plan)
    {
        return fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            None,
            CommandError::from_typed_service_error(error),
        );
    }
    if let Err(error) =
        cluster_service::write_linux_evidence_set_manifest(&job.case_root, &job.plan)
    {
        return fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            Some((connection, &job.plan.import_set_id, 0, 1)),
            CommandError::from_typed_service_error(error),
        );
    }
    if let Err(error) = cluster_service::update_linux_evidence_set_import_state(
        connection,
        &job.plan.import_set_id,
        "importing",
        0,
        0,
        None,
    ) {
        return fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            Some((connection, &job.plan.import_set_id, 0, 1)),
            CommandError::from_typed_service_error(error),
        );
    }
    Ok(())
}
