use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use app_services::import_scheduler::{
    global_import_group_gate, ImportGroupGateError, ImportGroupLease,
};
use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::CommandError;

use super::status::cancel_job;
use crate::events::event_bridge;

const IMPORT_GROUP_WAIT_TIMEOUT: Duration = Duration::from_secs(300);

pub(super) fn acquire_import_slot(
    job_repo: &JobRepo<'_>,
    job_id: &domain::JobId,
    app: Option<&AppHandle>,
    cancel_token: &Arc<AtomicBool>,
) -> Result<ImportGroupLease, CommandError> {
    if cancel_token.load(Ordering::Acquire) {
        cancel_job(
            job_repo,
            job_id,
            app,
            "Import cancelled while waiting for import slot",
        );
        return Err(CommandError::cancelled(
            "Import cancelled while waiting for import slot",
        ));
    }
    job_repo
        .update_progress(job_id, 2, "Waiting for import slot")
        .map_err(CommandError::from_typed_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, 2, "Waiting for import slot");
    }
    match global_import_group_gate().acquire(cancel_token, IMPORT_GROUP_WAIT_TIMEOUT) {
        Ok(lease) => {
            job_repo
                .update_progress(job_id, 5, "Import slot acquired")
                .map_err(CommandError::from_typed_service_error)?;
            if let Some(app) = app {
                event_bridge::emit_job_progress(app, &job_id.0, 5, "Import slot acquired");
            }
            Ok(lease)
        }
        Err(ImportGroupGateError::Cancelled) => {
            cancel_job(
                job_repo,
                job_id,
                app,
                "Import cancelled while waiting for import slot",
            );
            Err(CommandError::cancelled(
                "Import cancelled while waiting for import slot",
            ))
        }
        Err(ImportGroupGateError::TimedOut) => {
            let detail = format!(
                "Import admission timed out after {} seconds",
                IMPORT_GROUP_WAIT_TIMEOUT.as_secs()
            );
            job_repo
                .fail(job_id, &detail)
                .map_err(CommandError::from_typed_service_error)?;
            if let Some(app) = app {
                event_bridge::emit_job_failed(app, &job_id.0, &detail);
            }
            Err(CommandError::timeout(detail))
        }
    }
}
