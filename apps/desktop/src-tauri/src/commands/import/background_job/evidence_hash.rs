use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use app_services::hash_service::{evidence_jobs::load_hash_source, EvidenceHashError, HashService};
use domain::{DataSourceId, JobId};
use tauri::AppHandle;
use transport::CommandError;

use crate::events::event_bridge;

mod progress;
mod schedule;
mod status;

#[cfg(test)]
pub(super) use crate::state::TaskManager;
#[cfg(test)]
pub(super) use schedule::hash_task_id;
pub(crate) use schedule::schedule_pending_evidence_hashes;

use progress::{finish_progress_reporter, spawn_progress_reporter};
use status::{cancel_hash, complete_hash, fail_hash, fail_hash_setup};

static HASH_DB_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(super) fn hash_db_write_guard() -> std::sync::MutexGuard<'static, ()> {
    HASH_DB_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

pub(crate) fn run_background_evidence_hash(
    db_path: PathBuf,
    data_source_id: DataSourceId,
    job_id: JobId,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let case_root = db_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    if cancel_token.load(Ordering::Acquire) {
        cancel_hash(&connection, &case_root, &data_source_id, &job_id, app)?;
        return Ok(());
    }
    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job_id.0, "Evidence hash started");
        event_bridge::emit_job_progress(app, &job_id.0, 2, "Hashing evidence in background");
    }
    let source = match load_hash_source(&connection, &data_source_id) {
        Ok(source) => source,
        Err(error) => {
            return fail_hash_setup(
                &connection,
                &case_root,
                &data_source_id,
                &job_id,
                app,
                error,
            )
        }
    };
    let reporter = spawn_progress_reporter(&db_path, &data_source_id, &job_id, app);
    let progress_sender = reporter.as_ref().map(progress::ProgressReporter::sender);
    let hash_result = HashService::hash_evidence(
        &source.source_path,
        &source.kind,
        &cancel_token,
        &move |completed, total| {
            if let Some(sender) = &progress_sender {
                let _ = sender.try_send((completed, total));
            }
        },
    );
    finish_progress_reporter(reporter);
    match hash_result {
        Ok(result) => complete_hash(
            &connection,
            &case_root,
            &data_source_id,
            &job_id,
            app,
            &result,
        ),
        Err(EvidenceHashError::Cancelled) if cancel_token.load(Ordering::Acquire) => {
            cancel_hash(&connection, &case_root, &data_source_id, &job_id, app)
        }
        Err(error) => fail_hash(
            &connection,
            &case_root,
            &data_source_id,
            &job_id,
            app,
            error,
        ),
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/commands/import/evidence_hash.rs"]
mod tests;
